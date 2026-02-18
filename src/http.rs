use itertools::Itertools;
use rocket::{
    form::Form,
    request::FlashMessage,
    response::{Flash, Redirect},
    State,
};
use rocket_dyn_templates::Template;
use serde::Serialize;
use sqlx;

use crate::{
    config::ConfigState,
    export_traefik_config,
    https::HttpsRoute,
    traefik::{HttpConfig, HttpLoadBalancer, HttpRouter, HttpServer, HttpService},
    DbPool,
};

pub type DbResult<T> = Result<T, sqlx::Error>;

#[derive(Serialize, FromForm, Clone, Debug)]
#[serde(crate = "rocket::serde")]
pub struct HttpRoute {
    #[serde(skip_deserializing)]
    pub id: Option<i32>,
    pub enabled: bool,
    pub name: String,
    pub priority: Option<i32>,
    pub target: String,
    pub host_regex: bool,
    pub host: String,
    pub prefix: Option<String>,
}

impl HttpRoute {
    pub async fn count(conn: &DbPool) -> DbResult<i64> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM http_routes")
            .fetch_one(conn)
            .await?;
        Ok(count)
    }

    pub async fn all(conn: &DbPool) -> DbResult<Vec<HttpRoute>> {
        sqlx::query_as!(
            HttpRoute,
            r#"
            SELECT
                id as "id?: i32",
                enabled as "enabled: bool",
                name,
                priority as "priority?: i32",
                target,
                host_regex as "host_regex: bool",
                host,
                prefix as "prefix?: String"
            FROM http_routes
            "#
        )
        .fetch_all(conn)
        .await
    }

    pub async fn get(id: i32, conn: &DbPool) -> DbResult<HttpRoute> {
        sqlx::query_as!(
            HttpRoute,
            r#"
            SELECT
                id as "id?: i32",
                enabled as "enabled: bool",
                name,
                priority as "priority?: i32",
                target,
                host_regex as "host_regex: bool",
                host,
                prefix as "prefix?: String"
            FROM http_routes
            WHERE id = ?
            "#,
            id
        )
        .fetch_one(conn)
        .await
    }

    pub async fn insert(mut route: HttpRoute, conn: &DbPool) -> DbResult<u64> {
        route.cleanup();
        let result = sqlx::query!(
            "INSERT INTO http_routes (enabled, name, priority, target, host_regex, host, prefix) VALUES (?, ?, ?, ?, ?, ?, ?)",
            route.enabled,
            route.name,
            route.priority,
            route.target,
            route.host_regex,
            route.host,
            route.prefix
        )
        .execute(conn)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn update(id: i32, mut route: HttpRoute, conn: &DbPool) -> DbResult<u64> {
        route.cleanup();
        let result = sqlx::query!(
            "UPDATE http_routes SET enabled = ?, name = ?, priority = ?, target = ?, host_regex = ?, host = ?, prefix = ? WHERE id = ?",
            route.enabled,
            route.name,
            route.priority,
            route.target,
            route.host_regex,
            route.host,
            route.prefix,
            id
        )
        .execute(conn)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn delete(id: i32, conn: &DbPool) -> DbResult<u64> {
        let result = sqlx::query!("DELETE FROM http_routes WHERE id = ?", id)
            .execute(conn)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn enable(id: i32, enabled: bool, conn: &DbPool) -> DbResult<u64> {
        let result = sqlx::query!(
            "UPDATE http_routes SET enabled = ? WHERE id = ?",
            enabled,
            id
        )
        .execute(conn)
        .await?;
        Ok(result.rows_affected())
    }

    pub fn cleanup(&mut self) {
        if let Some(prefix) = &self.prefix {
            if prefix.trim().is_empty() {
                self.prefix = None;
            }
        }
    }

    pub async fn generate_traefik_config(conn: &DbPool) -> HttpConfig {
        let mut config = HttpConfig::new();

        let routes = HttpRoute::all(conn).await.unwrap();

        for mut route in routes {
            if route.enabled {
                route.cleanup();
                let router_name = format!("gui-http-{}-{}", route.id.unwrap(), route.name);

                let mut host_rule = if route.host_regex {
                    format!("HostRegexp(`{}`)", route.host.trim())
                } else {
                    let hosts = route
                        .host
                        .split(',')
                        .map(str::trim)
                        .map(|host| format!("Host(`{}`)", host))
                        .join(" && ");

                    format!("( {} )", hosts)
                };

                if let Some(prefix) = route.prefix {
                    host_rule = format!("({} && PathPrefix(`{}`))", host_rule, prefix);
                }

                config.routers.insert(
                    router_name.clone(),
                    HttpRouter {
                        priority: route.priority,
                        service: router_name.clone(),
                        rule: host_rule,
                        middlewares: Vec::new(),
                        tls: None,
                    },
                );

                config.services.insert(
                    router_name,
                    HttpService {
                        load_balancer: HttpLoadBalancer {
                            servers: vec![HttpServer { url: route.target }],
                        },
                    },
                );
            }
        }

        config
    }
}

#[derive(Serialize)]
struct Http {
    flash: Option<(String, String)>,
    routes: Vec<HttpRoute>,
    edit: Option<i32>,
}

impl Http {
    pub async fn raw(conn: &DbPool, flash: Option<(String, String)>, edit: Option<i32>) -> Self {
        match HttpRoute::all(conn).await {
            Ok(routes) => Self {
                flash,
                routes,
                edit,
            },
            Err(e) => {
                error!("DB error loading HTTP routes: {}", e);
                Self {
                    flash: Some(("error".into(), e.to_string())),
                    routes: Vec::new(),
                    edit: None,
                }
            }
        }
    }
}

#[get("/http?<edit>")]
pub async fn index(edit: Option<i32>, flash: Option<FlashMessage<'_>>, db: &State<DbPool>) -> Template {
    let flash = flash.map(FlashMessage::into_inner);
    Template::render("http", Http::raw(db.inner(), flash, edit).await)
}

#[post("/http", data = "<route_form>")]
pub async fn create(
    route_form: Form<HttpRoute>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let route = route_form.into_inner();

    if let Err(e) = HttpRoute::insert(route, db.inner()).await {
        Flash::error(Redirect::to("/http"), e.to_string())
    } else {
        export_traefik_config(db.inner(), &config.config()).await;
        Flash::success(Redirect::to("/http"), "Route created")
    }
}

#[post("/http/<id>", data = "<route_form>")]
pub async fn update(
    id: i32,
    route_form: Form<HttpRoute>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let route = route_form.into_inner();
    if let Err(e) = HttpRoute::update(id, route, db.inner()).await {
        Flash::error(Redirect::to("/http"), e.to_string())
    } else {
        export_traefik_config(db.inner(), &config.config()).await;
        Flash::success(Redirect::to("/http"), "Route updated")
    }
}

#[post("/http/<id>/enable", data = "<enabled>")]
pub async fn enable(
    id: i32,
    enabled: Form<bool>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let enabled = enabled.into_inner();
    if let Err(e) = HttpRoute::enable(id, enabled, db.inner()).await {
        Flash::error(Redirect::to("/http"), e.to_string())
    } else {
        export_traefik_config(db.inner(), &config.config()).await;
        Flash::success(Redirect::to("/http"), "Route updated")
    }
}

#[post("/http/<id>/delete", data = "<confirm>")]
pub async fn delete(
    id: i32,
    confirm: Form<bool>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    if confirm.into_inner() {
        if let Err(e) = HttpRoute::delete(id, db.inner()).await {
            Flash::error(Redirect::to("/http"), e.to_string())
        } else {
            export_traefik_config(db.inner(), &config.config()).await;
            Flash::success(Redirect::to("/http"), "Route deleted")
        }
    } else {
        Flash::error(Redirect::to("/http"), "Delete cancelled")
    }
}

#[post("/http/<id>/to_https", data = "<confirm>")]
pub async fn to_https(
    id: i32,
    confirm: Form<bool>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    if confirm.into_inner() {
        match HttpRoute::get(id, db.inner()).await {
            Ok(route) => {
                let new_route = HttpsRoute {
                    id: None,
                    enabled: route.enabled,
                    host: route.host,
                    host_regex: route.host_regex,
                    name: route.name,
                    prefix: route.prefix,
                    priority: route.priority,
                    target: route.target,
                    https_redirect: false,
                    allow_http_acme: false,
                };

                if let Err(e) = HttpsRoute::insert(new_route, db.inner()).await {
                    return Flash::error(Redirect::to("/http"), e.to_string());
                }

                if let Err(e) = HttpRoute::delete(id, db.inner()).await {
                    return Flash::error(Redirect::to("/http"), e.to_string());
                }

                export_traefik_config(db.inner(), &config.config()).await;
                Flash::success(Redirect::to("/https"), "Route converted")
            }
            Err(err) => Flash::error(Redirect::to("/http"), err.to_string()),
        }
    } else {
        Flash::error(Redirect::to("/http"), "Convertion cancelled")
    }
}
