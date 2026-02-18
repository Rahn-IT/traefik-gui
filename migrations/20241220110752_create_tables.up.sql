CREATE TABLE IF NOT EXISTS `http_routes`(
    `id` INTEGER PRIMARY KEY,
    `enabled` BOOL NOT NULL,
    `name` TEXT NOT NULL,
    `priority` INTEGER,
    `target` TEXT NOT NULL,
    `host_regex` BOOL NOT NULL,
    `host` TEXT NOT NULL,
    `prefix` TEXT
);

CREATE TABLE IF NOT EXISTS `tls_routes`(
    `id` INTEGER PRIMARY KEY,
    `enabled` BOOL NOT NULL,
    `name` TEXT NOT NULL,
    `priority` INTEGER,
    `target` TEXT NOT NULL,
    `host_regex` BOOL NOT NULL,
    `host` TEXT NOT NULL,
    `acme_http_passthrough` INTEGER,
    `https_redirect` BOOL NOT NULL
);
