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
    traefik::{
        HttpLoadBalancer, HttpRouter, HttpServer, HttpService, TcpLoadBalancer, TcpRouter,
        TcpServer, TcpService, TcpTls, TraefikConfig,
    },
    DbPool, ACME_PATH,
};

pub type DbResult<T> = Result<T, sqlx::Error>;

#[derive(Serialize, FromForm, Clone, Debug)]
#[serde(crate = "rocket::serde")]
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
    pub async fn count(conn: &DbPool) -> DbResult<i64> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM tls_routes")
            .fetch_one(conn)
            .await?;
        Ok(count)
    }

    pub async fn all(conn: &DbPool) -> DbResult<Vec<TlsRoute>> {
        sqlx::query_as!(
            TlsRoute,
            r#"
            SELECT
                id as "id?: i32",
                enabled as "enabled: bool",
                name,
                priority as "priority?: i32",
                target,
                host_regex as "host_regex: bool",
                host,
                acme_http_passthrough as "acme_http_passthrough?: i32",
                https_redirect as "https_redirect: bool"
            FROM tls_routes
            "#
        )
        .fetch_all(conn)
        .await
    }

    pub async fn insert(route: TlsRoute, conn: &DbPool) -> DbResult<u64> {
        let result = sqlx::query!(
            "INSERT INTO tls_routes (enabled, name, priority, target, host_regex, host, acme_http_passthrough, https_redirect) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            route.enabled,
            route.name,
            route.priority,
            route.target,
            route.host_regex,
            route.host,
            route.acme_http_passthrough,
            route.https_redirect
        )
        .execute(conn)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn update(id: i32, route: TlsRoute, conn: &DbPool) -> DbResult<u64> {
        let result = sqlx::query!(
            "UPDATE tls_routes SET enabled = ?, name = ?, priority = ?, target = ?, host_regex = ?, host = ?, acme_http_passthrough = ?, https_redirect = ? WHERE id = ?",
            route.enabled,
            route.name,
            route.priority,
            route.target,
            route.host_regex,
            route.host,
            route.acme_http_passthrough,
            route.https_redirect,
            id
        )
        .execute(conn)
        .await?;

        Ok(result.rows_affected())
    }

    pub async fn delete(id: i32, conn: &DbPool) -> DbResult<u64> {
        let result = sqlx::query!("DELETE FROM tls_routes WHERE id = ?", id)
            .execute(conn)
            .await?;
        Ok(result.rows_affected())
    }

    pub async fn enable(id: i32, enabled: bool, conn: &DbPool) -> DbResult<u64> {
        let result = sqlx::query!(
            "UPDATE tls_routes SET enabled = ? WHERE id = ?",
            enabled,
            id
        )
        .execute(conn)
        .await?;
        Ok(result.rows_affected())
    }

    pub async fn generate_traefik_config(conn: &DbPool) -> TraefikConfig {
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
                if target.rfind(':').is_none() {
                    target.push_str(":443");
                }

                config.tcp.services.insert(
                    router_name.clone(),
                    TcpService {
                        load_balancer: TcpLoadBalancer {
                            servers: vec![TcpServer { address: target }],
                        },
                    },
                );

                if let Some(acme_port) = route.acme_http_passthrough {
                    let mut acme_target = route.target.clone();
                    if let Some(pos) = acme_target.rfind(':') {
                        acme_target.replace_range(pos.., &format!(":{}", acme_port));
                    } else {
                        acme_target.push_str(&format!(":{}", acme_port));
                    }

                    let acme_router_name =
                        format!("gui-tls-{}-{}-acme", route.id.unwrap(), route.name);

                    let acme_rule = format!("({} && PathPrefix(`{}`))", http_host_rule, ACME_PATH);

                    config.http.routers.insert(
                        acme_router_name.clone(),
                        HttpRouter {
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
    pub async fn raw(conn: &DbPool, flash: Option<(String, String)>, edit: Option<i32>) -> Self {
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
pub async fn index(edit: Option<i32>, flash: Option<FlashMessage<'_>>, db: &State<DbPool>) -> Template {
    let flash = flash.map(FlashMessage::into_inner);
    Template::render("tls", Tls::raw(db.inner(), flash, edit).await)
}

#[post("/tls", data = "<route_form>")]
pub async fn create(
    route_form: Form<TlsRoute>,
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let route = route_form.into_inner();
    if let Err(e) = TlsRoute::insert(route, db.inner()).await {
        error!("DB error creating TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        export_traefik_config(db.inner(), &config.config()).await;
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
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    let route = route_form.into_inner();
    if let Err(e) = TlsRoute::update(id, route, db.inner()).await {
        error!("DB error updating TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        export_traefik_config(db.inner(), &config.config()).await;
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
    db: &State<DbPool>,
    config: &State<ConfigState>,
) -> Flash<Redirect> {
    if let Err(e) = TlsRoute::enable(id, enabled.into_inner(), db.inner()).await {
        error!("DB error updating TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        export_traefik_config(db.inner(), &config.config()).await;
        Flash::success(
            Redirect::to("/tls"),
            "Route updated successfully".to_string(),
        )
    }
}

#[post("/tls/<id>/delete")]
pub async fn delete(id: i32, db: &State<DbPool>, config: &State<ConfigState>) -> Flash<Redirect> {
    if let Err(e) = TlsRoute::delete(id, db.inner()).await {
        error!("DB error deleting TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        export_traefik_config(db.inner(), &config.config()).await;
        Flash::success(
            Redirect::to("/tls"),
            "Route deleted successfully".to_string(),
        )
    }
}
