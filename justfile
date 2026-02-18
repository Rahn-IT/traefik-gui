set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

# list commands also default
list:
    @just --list

# Setup the database
setup: create-db migrate

# Create the database
create-db:
    cargo sqlx database create

# Run database migrations
migrate:
    cargo sqlx migrate run

# remove database
clean:
    cargo sqlx database drop -y

# Restart the database by recreating it
restart: clean setup
