use diesel::{ExpressionMethods, QueryDsl, QueryResult, RunQueryDsl};
use rocket::{
    form::Form,
    request::FlashMessage,
    response::{Flash, Redirect},
};
use rocket_dyn_templates::Template;
use serde::Serialize;

use crate::{schema::tls_routes, DbConn};

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
pub async fn create(route_form: Form<TlsRoute>, conn: DbConn) -> Flash<Redirect> {
    let route = route_form.into_inner();
    if let Err(e) = TlsRoute::insert(route, &conn).await {
        error!("DB error creating TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        Flash::success(
            Redirect::to("/tls"),
            "Route created successfully".to_string(),
        )
    }
}

#[post("/tls/<id>", data = "<route_form>")]
pub async fn update(id: i32, route_form: Form<TlsRoute>, conn: DbConn) -> Flash<Redirect> {
    let route = route_form.into_inner();
    if let Err(e) = TlsRoute::update(id, route, &conn).await {
        error!("DB error updating TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        Flash::success(
            Redirect::to("/tls"),
            "Route updated successfully".to_string(),
        )
    }
}

#[post("/tls/<id>/enable", data = "<enabled>")]
pub async fn enable(id: i32, enabled: Form<bool>, conn: DbConn) -> Flash<Redirect> {
    if let Err(e) = TlsRoute::enable(id, enabled.into_inner(), &conn).await {
        error!("DB error updating TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        Flash::success(
            Redirect::to("/tls"),
            "Route updated successfully".to_string(),
        )
    }
}

#[post("/tls/<id>/delete")]
pub async fn delete(id: i32, conn: DbConn) -> Flash<Redirect> {
    if let Err(e) = TlsRoute::delete(id, &conn).await {
        error!("DB error deleting TLS route: {}", e);
        Flash::error(Redirect::to("/tls"), e.to_string())
    } else {
        Flash::success(
            Redirect::to("/tls"),
            "Route deleted successfully".to_string(),
        )
    }
}
