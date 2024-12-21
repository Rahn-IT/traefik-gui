use rocket::{
    fairing::AdHoc,
    fs::FileServer,
    request::FlashMessage,
    response::{Flash, Redirect},
    serde::Serialize,
    Build, Rocket,
};
use rocket_dyn_templates::Template;

#[macro_use]
extern crate rocket;
#[macro_use]
extern crate rocket_sync_db_pools;
#[macro_use]
extern crate diesel;

mod http;
mod schema;
mod tls;
mod traefik;

#[database("sqlite_database")]
pub struct DbConn(diesel::SqliteConnection);

#[launch]
async fn rocket() -> _ {
    rocket::build()
        .mount(
            "/",
            routes![
                index,
                redeploy,
                http::index,
                http::create,
                http::update,
                http::enable,
                http::delete,
                tls::index,
                tls::create,
                tls::update,
                tls::enable,
                tls::delete
            ],
        )
        .mount("/static", FileServer::from("templates/static"))
        .attach(Template::fairing())
        .attach(DbConn::fairing())
        .attach(AdHoc::on_ignite("Run Migrations", run_migrations))
        .attach(AdHoc::on_ignite(
            "Export Traefik Config",
            initialize_traefik_config,
        ))
}

async fn run_migrations(rocket: Rocket<Build>) -> Rocket<Build> {
    use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

    const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

    DbConn::get_one(&rocket)
        .await
        .expect("database connection")
        .run(|conn| {
            conn.run_pending_migrations(MIGRATIONS)
                .expect("diesel migrations");
        })
        .await;

    rocket
}

#[derive(Serialize)]
struct Index {
    flash: Option<(String, String)>,
    http_count: i64,
    tls_count: i64,
    config: String,
}

#[get("/")]
async fn index(conn: DbConn, flash: Option<FlashMessage<'_>>) -> Template {
    let http_count = http::HttpRoute::count(&conn).await.unwrap_or(0);
    let tls_count = tls::TlsRoute::count(&conn).await.unwrap_or(0);
    let config = generate_traefik_config(&conn).await;
    Template::render(
        "index",
        &Index {
            flash: flash.map(FlashMessage::into_inner),
            http_count,
            tls_count,
            config,
        },
    )
}

#[post("/redeploy")]
async fn redeploy(conn: DbConn) -> Flash<Redirect> {
    export_traefik_config(&conn).await;

    Flash::success(Redirect::to("/"), "Traefik config updated")
}

async fn generate_traefik_config(conn: &DbConn) -> String {
    let mut config = tls::TlsRoute::generate_traefik_config(conn).await;
    let http = http::HttpRoute::generate_traefik_config(conn).await;

    config.http.routers.extend(http.routers);
    config.http.services.extend(http.services);
    config.http.add_default_middlewares();

    let serialized = serde_yaml::to_string(&config).unwrap();

    serialized
}

pub async fn export_traefik_config(conn: &DbConn) {
    let config = generate_traefik_config(conn).await;

    std::fs::write("./traefik/gui.yml", config).unwrap();
}

async fn initialize_traefik_config(rocket: Rocket<Build>) -> Rocket<Build> {
    let conn = DbConn::get_one(&rocket).await.expect("database connection");

    export_traefik_config(&conn).await;

    rocket
}
