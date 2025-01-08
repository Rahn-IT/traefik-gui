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
    config::ConfigState,
    export_traefik_config,
    schema::tls_routes,
    traefik::{
        HttpLoadBalancer, HttpRouter, HttpServer, HttpService, TcpLoadBalancer, TcpRouter,
        TcpServer, TcpService, TcpTls, TraefikConfig,
    },
    DbConn,
};

#[derive(Serialize, Queryable, Insertable, AsChangeset, FromForm, Clone, Debug)]
#[serde(crate = "rocket::serde")]
#[diesel(table_name = tls_routes)]
pub struct TlsRoute {
    pub id: Option<i32>,
    pub enabled: bool,
    pub name: String,
    pub priority: Option<i32>,
    pub target: String,
    pub host_regex: bool,
    pub host: String,
    pub acme_http_passthrough: Option<i32>,
    pub https_redirect: bool,
}

impl TlsRoute {
    pub async fn count(conn: &DbConn) -> QueryResult<i64> {
        conn.run(|c| tls_routes::table.count().first::<i64>(c))
            .await
    }

    pub async fn all(conn: &crate::DbConn) -> QueryResult<Vec<TlsRoute>> {
        conn.run(|c| tls_routes::table.load::<TlsRoute>(c)).await
    }

    pub async fn insert(route: TlsRoute, conn: &DbConn) -> QueryResult<usize> {
        conn.run(move |c| {
            diesel::insert_into(tls_routes::table)
                .values(&route)
                .execute(c)
        })
        .await
    }

    pub async fn update(id: i32, route: TlsRoute, conn: &DbConn) -> QueryResult<usize> {
        conn.run(move |c| {
            diesel::update(tls_routes::table)
                .filter(tls_routes::id.eq(id))
                .set(&route)
                .execute(c)
        })
        .await
    }

    pub async fn delete(id: i32, conn: &DbConn) -> QueryResult<usize> {
        conn.run(move |c| {
            diesel::delete(tls_routes::table)
                .filter(tls_routes::id.eq(id))
                .execute(c)
        })
        .await
    }

    pub async fn enable(id: i32, enabled: bool, conn: &DbConn) -> QueryResult<usize> {
        conn.run(move |c| {
            diesel::update(tls_routes::table)
                .filter(tls_routes::id.eq(id))
                .set(tls_routes::enabled.eq(enabled))
                .execute(c)
        })
        .await
    }

    pub async fn generate_traefik_config(conn: &DbConn) -> TraefikConfig {
        let routes = TlsRoute::all(conn).await.unwrap();

        let mut config = TraefikConfig::new();

        for route in routes {
            if route.enabled {
                let router_name = format!("gui-tls-{}-{}", route.id.unwrap(), route.name);
                let host_rule = if route.host_regex {
                    format!("HostSNIRegexp(`{}`)", route.host)
                } else {
                    format!("HostSNI(`{}`)", route.host)
                };

                let http_host_rule = if route.host_regex {
                    format!("HostRegexp(`{}`)", route.host)
                } else {
                    format!("Host(`{}`)", route.host)
                };

                config.tcp.routers.insert(
                    router_name.clone(),
                    TcpRouter {
                        priority: route.priority,
                        service: router_name.clone(),
                        rule: host_rule,
                        tls: Some(TcpTls { passthrough: true }),
                    },
                );

                let mut target = route.target.clone();
                if target.rfind(':') == None {
                    target.push_str(":443");
                }

                config.tcp.services.insert(
                    router_name.clone(),
                    TcpService {
                        load_balancer: TcpLoadBalancer {
                            servers: vec![TcpServer {
                                address: format!("{}", target),
                            }],
                        },
                    },
                );

                if let Some(acme_port) = route.acme_http_passthrough {
                    // find the last colon in the target and replace the port after it with the acme port
                    let mut acme_target = route.target.clone();
                    if let Some(pos) = acme_target.rfind(':') {
                        acme_target.replace_range(pos.., &format!(":{}", acme_port));
                    } else {
                        acme_target.push_str(&format!(":{}", acme_port));
                    }

                    let acme_router_name =
                        format!("gui-tls-{}-{}-acme", route.id.unwrap(), route.name);

                    let acme_rule = format!(
                        "({} && PathPrefix(`/.well-known/acme-challenge/`))",
                        http_host_rule
                    );

                    config.http.routers.insert(
                        acme_router_name.clone(),
                        HttpRouter {
                            // make sure the acme router has a higher priority than the https redirect
                            priority: route.priority.map(|p| p + 1),
                            service: acme_router_name.clone(),
                            rule: acme_rule,
                            middlewares: Vec::new(),
                            tls: None,
                        },
                    );

                    config.http.services.insert(
                        acme_router_name.clone(),
                        HttpService {
                            load_balancer: HttpLoadBalancer {
                                servers: vec![HttpServer {
                                    url: format!("http://{}", acme_target),
                                }],
                            },
                        },
                    );
                }

                if route.https_redirect {
                    let redirect_router_name = format!("{}-redirect", router_name);

                    config.http.routers.insert(
                        redirect_router_name,
                        HttpRouter {
                            rule: http_host_rule,
                            service: "noop@internal".into(),
                            priority: route.priority,
                            middlewares: vec!["https-redirect".into()],
                            tls: None,
                        },
                    );
                }
            }
        }

        config
    }
}

#[derive(Serialize)]
struct Tls {
    flash: Option<(String, String)>,
    routes: Vec<TlsRoute>,
    edit: Option<i32>,
}

impl Tls {
    pub async fn raw(conn: &DbConn, flash: Option<(String, String)>, edit: Option<i32>) -> Self {
        match TlsRoute::all(conn).await {
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

#[get("/tls?<edit>")]
pub async fn index(edit: Option<i32>, flash: Option<FlashMessage<'_>>, conn: DbConn) -> Template {
    let flash = flash.map(FlashMessage::into_inner);
    Template::render("tls", Tls::raw(&conn, flash, edit).await)
}

#[post("/tls", data = "<route_form>")]
pub async fn create(
    route_form: Form<TlsRoute>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let route = route_form.into_inner();
    if let Err(e) = TlsRoute::insert(route, &conn).await {
        error!("DB error creating TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        export_traefik_config(&conn, &config.config()).await;
        Flash::success(
            Redirect::to("/tls"),
            "Route created successfully".to_string(),
        )
    }
}

#[post("/tls/<id>", data = "<route_form>")]
pub async fn update(
    id: i32,
    route_form: Form<TlsRoute>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let route = route_form.into_inner();
    if let Err(e) = TlsRoute::update(id, route, &conn).await {
        error!("DB error updating TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        export_traefik_config(&conn, &config.config()).await;
        Flash::success(
            Redirect::to("/tls"),
            "Route updated successfully".to_string(),
        )
    }
}

#[post("/tls/<id>/enable", data = "<enabled>")]
pub async fn enable(
    id: i32,
    enabled: Form<bool>,
    conn: DbConn,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    if let Err(e) = TlsRoute::enable(id, enabled.into_inner(), &conn).await {
        error!("DB error updating TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        export_traefik_config(&conn, &config.config()).await;
        Flash::success(
            Redirect::to("/tls"),
            "Route updated successfully".to_string(),
        )
    }
}

#[post("/tls/<id>/delete")]
pub async fn delete(id: i32, conn: DbConn, config: &State<ConfigState>) -> Flash<Redirect> {
    if let Err(e) = TlsRoute::delete(id, &conn).await {
        error!("DB error deleting TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        export_traefik_config(&conn, &config.config()).await;
        Flash::success(
            Redirect::to("/tls"),
            "Route deleted successfully".to_string(),
        )
    }
}
