use diesel::{ExpressionMethods, QueryDsl, QueryResult, RunQueryDsl};
use rocket::{
    form::Form,
    request::FlashMessage,
    response::{Flash, Redirect},
};
use rocket_dyn_templates::Template;
use serde::Serialize;

use crate::{schema::http_routes, DbConn};

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

    pub async fn insert(route: HttpRoute, conn: &DbConn) -> QueryResult<usize> {
        conn.run(move |c| {
            diesel::insert_into(http_routes::table)
                .values(&route)
                .execute(c)
        })
        .await
    }

    pub async fn update(id: i32, route: HttpRoute, conn: &DbConn) -> QueryResult<usize> {
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
pub async fn create(route_form: Form<HttpRoute>, conn: DbConn) -> Flash<Redirect> {
    let route = route_form.into_inner();

    // TODO: validate

    if let Err(e) = HttpRoute::insert(route, &conn).await {
        Flash::error(Redirect::to("/http"), e.to_string())
    } else {
        Flash::success(Redirect::to("/http"), "Route created")
    }
}

#[post("/http/<id>", data = "<route_form>")]
pub async fn update(id: i32, route_form: Form<HttpRoute>, conn: DbConn) -> Flash<Redirect> {
    // TODO: validate

    let route = route_form.into_inner();
    if let Err(e) = HttpRoute::update(id, route, &conn).await {
        Flash::error(Redirect::to("/http"), e.to_string())
    } else {
        Flash::success(Redirect::to("/http"), "Route updated")
    }
}

#[post("/http/<id>/enable", data = "<enabled>")]
pub async fn enable(id: i32, enabled: Form<bool>, conn: DbConn) -> Flash<Redirect> {
    let enabled = enabled.into_inner();
    if let Err(e) = HttpRoute::enable(id, enabled, &conn).await {
        Flash::error(Redirect::to("/http"), e.to_string())
    } else {
        Flash::success(Redirect::to("/http"), "Route updated")
    }
}

#[post("/http/<id>/delete", data = "<confirm>")]
pub async fn delete(id: i32, confirm: Form<bool>, conn: DbConn) -> Flash<Redirect> {
    if confirm.into_inner() {
        if let Err(e) = HttpRoute::delete(id, &conn).await {
            Flash::error(Redirect::to("/http"), e.to_string())
        } else {
            Flash::success(Redirect::to("/http"), "Route deleted")
        }
    } else {
        Flash::error(Redirect::to("/http"), "Delete cancelled")
    }
}
