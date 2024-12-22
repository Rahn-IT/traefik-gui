# Traefik GUI (V2)

> [!IMPORTANT]
>
> V2 is a complete rewrite and incompatible with V1
> 
> I've actually gotten some stars, so some people might be using this in the wild.
> I added a migration guide below so you know what changed and what you need to do to upgrade

I really like traefik, but with multiple VMs I need to bind come configs into the container so I can relay those connections.

This project is a Web-GUI for the Traefik reverse proxy. It allows you to easily add routes to your dynamic Traefik configuration.
It is meant for simple http and tcp routes, without having to manage the Traefik configuration manually.
This is especially useful if you only have terminal access.

I provided an installation guide a bit further below. If you need any help or are missing something, just open an issue

## V2 and Migration

V2 is a rewrite which adds a few new features, and generally improves the experience (hopefully)

Improvements over V1:
- one-click HTTPS redirect for TLS routes
- allow editing routes (finally)
- quickly disable and re-enable routes without deleting them
- Nice UI for adding a PathPrefix

### Migration

To migrate you will need to do the following:
- write down your old routes (a screenshot should do too)
- delete the old config volume or bind mount for `/app/data`
- create a new volume or mount for `/app/db` 
- delete the old generated traefik config file. If it's the only file in the volume, you can just recreate the volume
- replace the image with the new one: `ghcr.io/rahn-it/traefik-gui:master`
- replace the container port. The UI is using port `8000` instead of `3000` now. You may also just replace the internal port and leave the external mapping at `3000`.
- open the new UI and re-add your routes.

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

# Installation

Traefik-GUI can be installed using docker:

```shell
docker pull ghcr.io/rahn-it/traefik-gui:master
docker run -d -p 8000:8000 --name traefik-gui -v ./db:/app/db -v ./traefik-configs:/app/traefik ghcr.io/rahn-it/traefik-gui:master
```

I would recommend using docker-compose though.

As a starting point you can use the [docker compose file](docker-compose.yaml) frm this repository.
Don't forget to enter your email. The example will spin up the traefik dashboard on port 8080

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
