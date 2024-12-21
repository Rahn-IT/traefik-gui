use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Serialize)]
pub struct TraefikConfig {
    #[serde(skip_serializing_if = "HttpConfig::is_empty")]
    pub http: HttpConfig,
    #[serde(skip_serializing_if = "TcpConfig::is_empty")]
    pub tcp: TcpConfig,
}

impl TraefikConfig {
    pub fn new() -> Self {
        Self {
            http: HttpConfig::new(),
            tcp: TcpConfig::new(),
        }
    }
}

#[derive(Serialize)]
pub struct HttpConfig {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub routers: BTreeMap<String, HttpRouter>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub services: BTreeMap<String, HttpService>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub middlewares: BTreeMap<String, HttpMiddleware>,
}

impl HttpConfig {
    pub fn new() -> Self {
        Self {
            routers: BTreeMap::new(),
            services: BTreeMap::new(),
            middlewares: BTreeMap::new(),
        }
    }

    pub fn merge(&mut self, other: HttpConfig) {
        self.routers.extend(other.routers);
        self.services.extend(other.services);
        self.middlewares.extend(other.middlewares);
    }

    pub fn add_default_middlewares(&mut self) {
        self.middlewares.insert(
            "https-redirect".into(),
            HttpMiddleware {
                redirect_scheme: Some(HttpRedirectScheme {
                    scheme: HttpScheme::Https,
                }),
            },
        );
    }

    pub fn is_empty(&self) -> bool {
        self.routers.is_empty() && self.services.is_empty() && self.middlewares.is_empty()
    }
}

#[derive(Serialize)]
pub struct HttpRouter {
    pub rule: String,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub middlewares: Vec<String>,
}

#[derive(Serialize)]
pub struct HttpService {
    #[serde(rename = "loadBalancer")]
    pub load_balancer: HttpLoadBalancer,
}

#[derive(Serialize)]
pub struct HttpLoadBalancer {
    pub servers: Vec<HttpServer>,
}

#[derive(Serialize)]
pub struct HttpServer {
    pub url: String,
}

#[derive(Serialize)]
pub struct HttpMiddleware {
    #[serde(rename = "redirectScheme")]
    redirect_scheme: Option<HttpRedirectScheme>,
}

#[derive(Serialize)]
pub struct HttpRedirectScheme {
    scheme: HttpScheme,
}

#[derive(Serialize)]
pub enum HttpScheme {
    #[serde(rename = "http")]
    _Http,
    #[serde(rename = "https")]
    Https,
}

#[derive(Serialize)]
pub struct TcpConfig {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub routers: BTreeMap<String, TcpRouter>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub services: BTreeMap<String, TcpService>,
}

impl TcpConfig {
    pub fn new() -> Self {
        Self {
            routers: BTreeMap::new(),
            services: BTreeMap::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.routers.is_empty() && self.services.is_empty()
    }
}

#[derive(Serialize)]
pub struct TcpRouter {
    pub rule: String,
    pub service: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub priority: Option<i32>,
    pub tls: Option<TcpTls>,
}

#[derive(Serialize)]
pub struct TcpTls {
    pub passthrough: bool,
}

#[derive(Serialize)]
pub struct TcpService {
    #[serde(rename = "loadBalancer")]
    pub load_balancer: TcpLoadBalancer,
}

#[derive(Serialize)]
pub struct TcpLoadBalancer {
    pub servers: Vec<TcpServer>,
}

#[derive(Serialize)]
pub struct TcpServer {
    pub address: String,
}
