use std::sync::Mutex;

use rocket::{
    form::Form,
    request::FlashMessage,
    response::{Flash, Redirect},
    State,
};
use rocket_dyn_templates::Template;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub struct ConfigState {
    config: Mutex<Config>,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("error opening config file: {0}")]
    Load(#[from] std::io::Error),
    #[error("error parsing config file: {0}")]
    Json(#[from] serde_yaml::Error),
    #[error("error saving config file: {0}")]
    Save(std::io::Error),
}

impl ConfigState {
    pub fn load() -> Result<Self, ConfigError> {
        // check if file exists, if not, create it
        if !std::path::Path::new("./db/config.yaml").exists() {
            let default_config = Config::default();
            std::fs::write("./db/config.yaml", serde_yaml::to_string(&default_config)?)
                .map_err(ConfigError::Save)?;
        }

        let serialized = std::fs::read_to_string("./db/config.yaml")?;

        let config: Config = serde_yaml::from_str(&serialized)?;

        Ok(Self {
            config: Mutex::new(config),
        })
    }

    pub fn save(&self, config: Config) -> Result<(), ConfigError> {
        let serialized = serde_yaml::to_string(&config)?;

        std::fs::write("./db/config.yaml", serialized).map_err(ConfigError::Save)?;

        let mut current = self.config.lock().unwrap();
        *current = config;

        Ok(())
    }

    pub fn config(&self) -> Config {
        self.config.lock().unwrap().clone()
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, FromForm)]
pub struct Config {
    pub acme_provider_name: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            acme_provider_name: "".into(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ConfigRender {
    pub config: Config,
    pub flash: Option<(String, String)>,
}

#[get("/config")]
pub async fn index(state: &State<ConfigState>, flash: Option<FlashMessage<'_>>) -> Template {
    let config = state.config();
    let flash = flash.map(FlashMessage::into_inner);

    Template::render("config", ConfigRender { config, flash })
}

#[post("/config", data = "<config>")]
pub async fn update(state: &State<ConfigState>, config: Form<Config>) -> Flash<Redirect> {
    let config = config.into_inner();

    state.save(config).unwrap();

    Flash::success(Redirect::to("/config"), "Config updated")
}
