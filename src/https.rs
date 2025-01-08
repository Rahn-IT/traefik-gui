use diesel::{ExpressionMethods, QueryDsl, QueryResult, RunQueryDsl};
use rocket::{
    form::Form,
    request::FlashMessage,
    response::{Flash, Redirect},
    State,
};
use rocket_dyn_templates::Template;
use serde::Serialize;

use crate::{
    config::{Config, ConfigState},
    export_traefik_config,
    schema::https_routes,
    traefik::{HttpConfig, HttpLoadBalancer, HttpRouter, HttpServer, HttpService, HttpTls},
    DbConn,
};

#[derive(Serialize, Queryable, Insertable, AsChangeset, FromForm, Clone, Debug)]
#[serde(crate = "rocket::serde")]
#[diesel(table_name = https_routes)]
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
}

impl HttpsRoute {
    pub async fn count(conn: &DbConn) -> QueryResult<i64> {
        conn.run(|c| https_routes::table.count().first::<i64>(c))
            .await
    }

    pub async fn all(conn: &DbConn) -> QueryResult<Vec<HttpsRoute>> {
        conn.run(|c| https_routes::table.load::<HttpsRoute>(c))
            .await
    }

    pub async fn insert(mut route: HttpsRoute, conn: &DbConn) -> QueryResult<usize> {
        route.cleanup();
        conn.run(move |c| {
            diesel::insert_into(https_routes::table)
                .values(&route)
                .execute(c)
        })
        .await
    }

    pub async fn update(id: i32, mut route: HttpsRoute, conn: &DbConn) -> QueryResult<usize> {
        route.cleanup();
        conn.run(move |c| {
            diesel::update(https_routes::table)
                .filter(https_routes::id.eq(id))
                .set(&route)
                .execute(c)
        })
        .await
    }

    pub async fn delete(id: i32, conn: &DbConn) -> QueryResult<usize> {
        conn.run(move |c| {
            diesel::delete(https_routes::table)
                .filter(https_routes::id.eq(id))
                .execute(c)
        })
        .await
    }

    pub async fn enable(id: i32, enabled: bool, conn: &DbConn) -> QueryResult<usize> {
        conn.run(move |c| {
            diesel::update(https_routes::table)
                .filter(https_routes::id.eq(id))
                .set(https_routes::enabled.eq(enabled))
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

    pub async fn generate_traefik_config(conn: &DbConn, config: &Config) -> HttpConfig {
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

                let mut host_rule = if route.host_regex {
                    format!("HostRegexp(`{}`)", route.host)
                } else {
                    format!("Host(`{}`)", route.host)
                };

                if let Some(prefix) = route.prefix {
                    host_rule = format!("({} && PathPrefix(`{}`))", host_rule, prefix);
                }

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
                            servers: vec![{ HttpServer { url: route.target } }],
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
    pub async fn raw(conn: &DbConn, flash: Option<(String, String)>, edit: Option<i32>) -> Self {
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
pub async fn index(edit: Option<i32>, flash: Option<FlashMessage<'_>>, conn: DbConn) -> Template {
    let flash = flash.map(FlashMessage::into_inner);
    Template::render("https", Https::raw(&conn, flash, edit).await)
}

#[post("/https", data = "<route_form>")]
pub async fn create(
    route_form: Form<HttpsRoute>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let route = route_form.into_inner();

    // TODO: validate

    if let Err(e) = HttpsRoute::insert(route, &conn).await {
        Flash::error(Redirect::to("/https"), e.to_string())
    } else {
        export_traefik_config(&conn, &config.config()).await;
        Flash::success(Redirect::to("/https"), "Route created")
    }
}

#[post("/https/<id>", data = "<route_form>")]
pub async fn update(
    id: i32,
    route_form: Form<HttpsRoute>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    // TODO: validate

    let route = route_form.into_inner();
    if let Err(e) = HttpsRoute::update(id, route, &conn).await {
        Flash::error(Redirect::to("/https"), e.to_string())
    } else {
        export_traefik_config(&conn, &config.config()).await;
        Flash::success(Redirect::to("/https"), "Route updated")
    }
}

#[post("/https/<id>/enable", data = "<enabled>")]
pub async fn enable(
    id: i32,
    enabled: Form<bool>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let enabled = enabled.into_inner();
    if let Err(e) = HttpsRoute::enable(id, enabled, &conn).await {
        Flash::error(Redirect::to("/https"), e.to_string())
    } else {
        export_traefik_config(&conn, &config.config()).await;
        Flash::success(Redirect::to("/https"), "Route updated")
    }
}

#[post("/https/<id>/delete", data = "<confirm>")]
pub async fn delete(
    id: i32,
    confirm: Form<bool>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    if confirm.into_inner() {
        if let Err(e) = HttpsRoute::delete(id, &conn).await {
            Flash::error(Redirect::to("/https"), e.to_string())
        } else {
            export_traefik_config(&conn, &config.config()).await;
            Flash::success(Redirect::to("/https"), "Route deleted")
        }
    } else {
        Flash::error(Redirect::to("/https"), "Delete cancelled")
    }
}
