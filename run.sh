docker run -d --name cargo-bisect-rustc-service --net=internal --restart=always  \
 "-v=/apps/cargo-bisect-rustc-service/db:/app/db" \
 "-e=SQLITE_DB=/app/db/db.sqlite" \
  docker.nilstrieb.dev/cargo-bisect-rustc-service:1.3