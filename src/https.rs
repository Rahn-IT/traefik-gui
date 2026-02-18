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
    config::{Config, ConfigState},
    export_traefik_config,
    http::HttpRoute,
    traefik::{HttpConfig, HttpLoadBalancer, HttpRouter, HttpServer, HttpService, HttpTls},
    DbPool, ACME_PATH,
};

pub type DbResult<T> = Result<T, sqlx::Error>;

#[derive(Serialize, FromForm, Clone, Debug)]
#[serde(crate = "rocket::serde")]
pub struct HttpsRoute {
    #[serde(skip_deserializing)]
    pub id: Option<i32>,
    pub enabled: bool,
    pub name: String,
    pub priority: Option<i32>,
    pub target: String,
    pub host_regex: bool,
    pub host: String,
    pub prefix: Option<String>,
    pub https_redirect: bool,
    pub allow_http_acme: bool,
}

impl HttpsRoute {
    pub async fn count(conn: &DbPool) -> DbResult<i64> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM https_routes")
            .fetch_one(conn)
            .await?;
        Ok(count)
    }

    pub async fn all(conn: &DbPool) -> DbResult<Vec<HttpsRoute>> {
        sqlx::query_as!(
            HttpsRoute,
            r#"
            SELECT
                id as "id?: i32",
                enabled as "enabled: bool",
                name,
                priority as "priority?: i32",
                target,
                host_regex as "host_regex: bool",
                host,
                prefix as "prefix?: String",
                https_redirect as "https_redirect: bool",
                allow_http_acme as "allow_http_acme: bool"
            FROM https_routes
            "#
        )
        .fetch_all(conn)
        .await
    }

    pub async fn get(id: i32, conn: &DbPool) -> DbResult<HttpsRoute> {
        sqlx::query_as!(
            HttpsRoute,
            r#"
            SELECT
                id as "id?: i32",
                enabled as "enabled: bool",
                name,
                priority as "priority?: i32",
                target,
                host_regex as "host_regex: bool",
                host,
                prefix as "prefix?: String",
                https_redirect as "https_redirect: bool",
                allow_http_acme as "allow_http_acme: bool"
            FROM https_routes
            WHERE id = ?
            "#,
            id
        )
        .fetch_one(conn)
        .await
    }

    pub async fn insert(mut route: HttpsRoute, conn: &DbPool) -> DbResult<u64> {
        route.cleanup();
        let result = sqlx::query!(
            "INSERT INTO https_routes (enabled, name, priority, target, host_regex, host, prefix, https_redirect, allow_http_acme) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            route.enabled,
            route.name,
            route.priority,
            route.target,
            route.host_regex,
            route.host,
            route.prefix,
            route.https_redirect,
            route.allow_http_acme
        )
        .execute(conn)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn update(id: i32, mut route: HttpsRoute, conn: &DbPool) -> DbResult<u64> {
        route.cleanup();
        let result = sqlx::query!(
            "UPDATE https_routes SET enabled = ?, name = ?, priority = ?, target = ?, host_regex = ?, host = ?, prefix = ?, https_redirect = ?, allow_http_acme = ? WHERE id = ?",
            route.enabled,
            route.name,
            route.priority,
            route.target,
            route.host_regex,
            route.host,
            route.prefix,
            route.https_redirect,
            route.allow_http_acme,
            id
        )
        .execute(conn)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn delete(id: i32, conn: &DbPool) -> DbResult<u64> {
        let result = sqlx::query!("DELETE FROM https_routes WHERE id = ?", id)
            .execute(conn)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn enable(id: i32, enabled: bool, conn: &DbPool) -> DbResult<u64> {
        let result = sqlx::query!(
            "UPDATE https_routes SET enabled = ? WHERE id = ?",
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

    pub async fn generate_traefik_config(conn: &DbPool, config: &Config) -> HttpConfig {
        let mut traefik_config = HttpConfig::new();

        let routes = HttpsRoute::all(conn).await.unwrap();

        let acme_provider = if config.acme_provider_name.is_empty() {
            None
        } else {
            Some(config.acme_provider_name.clone())
        };

        for mut route in routes {
            if route.enabled {
                route.cleanup();
                let router_name = format!("gui-https-{}-{}", route.id.unwrap(), route.name);

                let base_rule = if route.host_regex {
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

                let host_rule = if let Some(prefix) = route.prefix {
                    format!("({} && PathPrefix(`{}`))", base_rule, prefix)
                } else {
                    base_rule.clone()
                };

                if route.https_redirect {
                    let redirect_router_name = format!("{}-redirect", router_name);

                    traefik_config.routers.insert(
                        redirect_router_name,
                        HttpRouter {
                            rule: host_rule.clone(),
                            service: "noop@internal".into(),
                            priority: route.priority,
                            middlewares: vec!["https-redirect".into()],
                            tls: None,
                        },
                    );
                }

                if route.allow_http_acme {
                    let acme_router_name = format!("{}-acme", router_name);
                    let acme_rule = format!("({} && PathPrefix(`{}`))", base_rule, ACME_PATH);

                    traefik_config.routers.insert(
                        acme_router_name,
                        HttpRouter {
                            rule: acme_rule,
                            service: router_name.clone(),
                            priority: route.priority,
                            middlewares: Vec::new(),
                            tls: None,
                        },
                    );
                }

                traefik_config.routers.insert(
                    router_name.clone(),
                    HttpRouter {
                        priority: route.priority,
                        service: router_name.clone(),
                        rule: host_rule,
                        middlewares: Vec::new(),
                        tls: Some(HttpTls {
                            cert_resolver: acme_provider.clone(),
                        }),
                    },
                );

                traefik_config.services.insert(
                    router_name,
                    HttpService {
                        load_balancer: HttpLoadBalancer {
                            servers: vec![HttpServer { url: route.target }],
                        },
                    },
                );
            }
        }

        traefik_config
    }
}

#[derive(Serialize)]
struct Https {
    flash: Option<(String, String)>,
    routes: Vec<HttpsRoute>,
    edit: Option<i32>,
}

impl Https {
    pub async fn raw(conn: &DbPool, flash: Option<(String, String)>, edit: Option<i32>) -> Self {
        match HttpsRoute::all(conn).await {
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

#[get("/https?<edit>")]
pub async fn index(edit: Option<i32>, flash: Option<FlashMessage<'_>>, db: &State<DbPool>) -> Template {
    let flash = flash.map(FlashMessage::into_inner);
    Template::render("https", Https::raw(db.inner(), flash, edit).await)
}

#[post("/https", data = "<route_form>")]
pub async fn create(
    route_form: Form<HttpsRoute>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let route = route_form.into_inner();

    if let Err(e) = HttpsRoute::insert(route, db.inner()).await {
        Flash::error(Redirect::to("/https"), e.to_string())
    } else {
        export_traefik_config(db.inner(), &config.config()).await;
        Flash::success(Redirect::to("/https"), "Route created")
    }
}

#[post("/https/<id>", data = "<route_form>")]
pub async fn update(
    id: i32,
    route_form: Form<HttpsRoute>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let route = route_form.into_inner();
    if let Err(e) = HttpsRoute::update(id, route, db.inner()).await {
        Flash::error(Redirect::to("/https"), e.to_string())
    } else {
        export_traefik_config(db.inner(), &config.config()).await;
        Flash::success(Redirect::to("/https"), "Route updated")
    }
}

#[post("/https/<id>/enable", data = "<enabled>")]
pub async fn enable(
    id: i32,
    enabled: Form<bool>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let enabled = enabled.into_inner();
    if let Err(e) = HttpsRoute::enable(id, enabled, db.inner()).await {
        Flash::error(Redirect::to("/https"), e.to_string())
    } else {
        export_traefik_config(db.inner(), &config.config()).await;
        Flash::success(Redirect::to("/https"), "Route updated")
    }
}

#[post("/https/<id>/delete", data = "<confirm>")]
pub async fn delete(
    id: i32,
    confirm: Form<bool>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    if confirm.into_inner() {
        if let Err(e) = HttpsRoute::delete(id, db.inner()).await {
            Flash::error(Redirect::to("/https"), e.to_string())
        } else {
            export_traefik_config(db.inner(), &config.config()).await;
            Flash::success(Redirect::to("/https"), "Route deleted")
        }
    } else {
        Flash::error(Redirect::to("/https"), "Delete cancelled")
    }
}

#[post("/https/<id>/to_http", data = "<confirm>")]
pub async fn to_http(
    id: i32,
    confirm: Form<bool>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    if confirm.into_inner() {
        match HttpsRoute::get(id, db.inner()).await {
            Ok(route) => {
                let new_route = HttpRoute {
                    id: None,
                    enabled: route.enabled,
                    host: route.host,
                    host_regex: route.host_regex,
                    name: route.name,
                    prefix: route.prefix,
                    priority: route.priority,
                    target: route.target,
                };

                if let Err(e) = HttpRoute::insert(new_route, db.inner()).await {
                    return Flash::error(Redirect::to("/https"), e.to_string());
                }

                if let Err(e) = HttpsRoute::delete(id, db.inner()).await {
                    return Flash::error(Redirect::to("/https"), e.to_string());
                }

                export_traefik_config(db.inner(), &config.config()).await;
                Flash::success(Redirect::to("/http"), "Route converted")
            }
            Err(err) => Flash::error(Redirect::to("/https"), err.to_string()),
        }
    } else {
        Flash::error(Redirect::to("/https"), "Convertion cancelled")
    }
}
