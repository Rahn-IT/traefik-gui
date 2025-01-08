// @generated automatically by Diesel CLI.

diesel::table! {
    http_routes (id) {
        id -> Nullable<Integer>,
        enabled -> Bool,
        name -> Text,
        priority -> Nullable<Integer>,
        target -> Text,
        host_regex -> Bool,
        host -> Text,
        prefix -> Nullable<Text>,
    }
}

diesel::table! {
    tls_routes (id) {
        id -> Nullable<Integer>,
        enabled -> Bool,
        name -> Text,
        priority -> Nullable<Integer>,
        target -> Text,
        host_regex -> Bool,
        host -> Text,
        acme_http_passthrough -> Nullable<Integer>,
        https_redirect -> Bool,
    }
}

diesel::table! {
    https_routes (id) {
        id -> Nullable<Integer>,
        enabled -> Bool,
        name -> Text,
        priority -> Nullable<Integer>,
        target -> Text,
        host_regex -> Bool,
        host -> Text,
        prefix -> Nullable<Text>,
        https_redirect -> Bool,
    }
}

diesel::allow_tables_to_appear_in_same_query!(http_routes, tls_routes,);
