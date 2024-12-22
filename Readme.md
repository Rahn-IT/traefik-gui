# Traefik GUI (V2)

I really like traefik, but with multiple VMs I need to bind come configs into the container so I can relay those connections.

This project is a Web-GUI for the Traefik reverse proxy. It allows you to easily add routes to your dynamic Traefik configuration.

It is meant for simple http and tcp routes, without having to manage the Traefik configuration manually.

This is especially useful if you only have terminal access.

## Screenshots

![Screenshot](screenshots/home.png)
![Screenshot](screenshots/http.png)
![Screenshot](screenshots/edit.png)
![Screenshot](screenshots/tls.png)

## Features

Currently, Traefik-gui has the following features:

Forward HTTP-Request:
- By Hostname
- By Host regex
- add additional Path Prefix

Forward TLS Requests
- By Hostname (SNI)
- By Regex (SNI)
- Automatically add HTTP -> HTTPS redirect
- Automatically add HTTP rule for the `/.well-known/acme-challenge/` endpoints - when set to port 80 your downstream application can request Let's encrypt certificates via HTTP.

The GUI currently doesn't validate the data you put in. It'ss just paste the incorrect data in the config file.

## Usage

You can access the GUI at port 8000. e.g.: http://localhost:8000

The tool will automatically generate the Traefik configuration and put it in the `/app/traefik` folder inside the container.
The configuration is saved using sqlite inside the `/app/db` folder.

When using the docker compose example, this folder will already be connected to the traefik container.

If you have any questions or problems, you're welcome to create an issue :)

# Attribution

This project is licensed under the [AGPL-3.0](LICENSE).

Developed by [Rahn IT](https://it-rahn.de/).

Thanks to the great people of [Traefik](https://traefik.io/), [Rocket](https://rocket.rs/) and everyone who made this possible.
