-- Your SQL goes here
CREATE TABLE `https_routes`(
	`id` INTEGER PRIMARY KEY,
	`enabled` BOOL NOT NULL,
	`name` TEXT NOT NULL,
	`priority` INTEGER,
	`target` TEXT NOT NULL,
	`host_regex` BOOL NOT NULL,
	`host` TEXT NOT NULL,
	`prefix` TEXT,
	`https_redirect` BOOL NOT NULL
);

