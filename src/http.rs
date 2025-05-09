use diesel::{ExpressionMethods, QueryDsl, QueryResult, RunQueryDsl};
use itertools::Itertools;
use rocket::{
    form::Form,
    request::FlashMessage,
    response::{Flash, Redirect},
    State,
};
use rocket_dyn_templates::Template;
use serde::Serialize;

use crate::{
    config::ConfigState,
    export_traefik_config,
    https::HttpsRoute,
    schema::http_routes::{self, dsl},
    traefik::{HttpConfig, HttpLoadBalancer, HttpRouter, HttpServer, HttpService},
    DbConn,
};

#[derive(Serialize, Queryable, Insertable, AsChangeset, FromForm, Clone, Debug)]
#[serde(crate = "rocket::serde")]
#[diesel(table_name = http_routes)]
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
    pub async fn count(conn: &DbConn) -> QueryResult<i64> {
        conn.run(|c| http_routes::table.count().first::<i64>(c))
            .await
    }

    pub async fn all(conn: &DbConn) -> QueryResult<Vec<HttpRoute>> {
        conn.run(|c| http_routes::table.load::<HttpRoute>(c)).await
    }

    pub async fn get(id: i32, conn: &DbConn) -> QueryResult<HttpRoute> {
        conn.run(move |c| http_routes::table.filter(dsl::id.eq(id)).first(c))
            .await
    }

    pub async fn insert(mut route: HttpRoute, conn: &DbConn) -> QueryResult<usize> {
        route.cleanup();
        conn.run(move |c| {
            diesel::insert_into(http_routes::table)
                .values(&route)
                .execute(c)
        })
        .await
    }

    pub async fn update(id: i32, mut route: HttpRoute, conn: &DbConn) -> QueryResult<usize> {
        route.cleanup();
        conn.run(move |c| {
            diesel::update(http_routes::table)
                .filter(http_routes::id.eq(id))
                .set(&route)
                .execute(c)
        })
        .await
    }

    pub async fn delete(id: i32, conn: &DbConn) -> QueryResult<usize> {
        conn.run(move |c| {
            diesel::delete(http_routes::table)
                .filter(http_routes::id.eq(id))
                .execute(c)
        })
        .await
    }

    pub async fn enable(id: i32, enabled: bool, conn: &DbConn) -> QueryResult<usize> {
        conn.run(move |c| {
            diesel::update(http_routes::table)
                .filter(http_routes::id.eq(id))
                .set(http_routes::enabled.eq(enabled))
                .execute(c)
        })
        .await
    }

    pub fn cleanup(&mut self) {
        if let Some(prefix) = &self.prefix {
            if prefix.trim().is_empty() {
                self.prefix = None;
            }
        }
    }

    pub async fn generate_traefik_config(conn: &DbConn) -> HttpConfig {
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
                            servers: vec![{ HttpServer { url: route.target } }],
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
    pub async fn raw(conn: &DbConn, flash: Option<(String, String)>, edit: Option<i32>) -> Self {
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
pub async fn index(edit: Option<i32>, flash: Option<FlashMessage<'_>>, conn: DbConn) -> Template {
    let flash = flash.map(FlashMessage::into_inner);
    Template::render("http", Http::raw(&conn, flash, edit).await)
}

#[post("/http", data = "<route_form>")]
pub async fn create(
    route_form: Form<HttpRoute>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let route = route_form.into_inner();

    // TODO: validate

    if let Err(e) = HttpRoute::insert(route, &conn).await {
        Flash::error(Redirect::to("/http"), e.to_string())
    } else {
        export_traefik_config(&conn, &config.config()).await;
        Flash::success(Redirect::to("/http"), "Route created")
    }
}

#[post("/http/<id>", data = "<route_form>")]
pub async fn update(
    id: i32,
    route_form: Form<HttpRoute>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    // TODO: validate

    let route = route_form.into_inner();
    if let Err(e) = HttpRoute::update(id, route, &conn).await {
        Flash::error(Redirect::to("/http"), e.to_string())
    } else {
        export_traefik_config(&conn, &config.config()).await;
        Flash::success(Redirect::to("/http"), "Route updated")
    }
}

#[post("/http/<id>/enable", data = "<enabled>")]
pub async fn enable(
    id: i32,
    enabled: Form<bool>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let enabled = enabled.into_inner();
    if let Err(e) = HttpRoute::enable(id, enabled, &conn).await {
        Flash::error(Redirect::to("/http"), e.to_string())
    } else {
        export_traefik_config(&conn, &config.config()).await;
        Flash::success(Redirect::to("/http"), "Route updated")
    }
}

#[post("/http/<id>/delete", data = "<confirm>")]
pub async fn delete(
    id: i32,
    confirm: Form<bool>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    if confirm.into_inner() {
        if let Err(e) = HttpRoute::delete(id, &conn).await {
            Flash::error(Redirect::to("/http"), e.to_string())
        } else {
            export_traefik_config(&conn, &config.config()).await;
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
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    if confirm.into_inner() {
        match HttpRoute::get(id, &conn).await {
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

                if let Err(e) = HttpsRoute::insert(new_route, &conn).await {
                    return Flash::error(Redirect::to("/http"), e.to_string());
                }

                if let Err(e) = HttpRoute::delete(id, &conn).await {
                    return Flash::error(Redirect::to("/http"), e.to_string());
                }

                export_traefik_config(&conn, &config.config()).await;
                Flash::success(Redirect::to("/https"), "Route converted")
            }
            Err(err) => Flash::error(Redirect::to("/http"), err.to_string()),
        }
    } else {
        Flash::error(Redirect::to("/http"), "Convertion cancelled")
    }
}
