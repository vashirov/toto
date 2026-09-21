# podman

## List running containers

Show all running containers with their names, status, and ports.

```sh
podman ps --format "table {{.ID}}\t{{.Names}}\t{{.Status}}\t{{.Ports}}"
```

## Stop a container

```sh
podman stop <container>
```

$ container: podman ps --format '{{.Names}}' --- --prevent-extra

## View container logs

Follow log output in real-time. Use Ctrl-C to stop.

```sh
podman logs -f --tail <lines> <container>
```

$ lines: echo -e "50\n100\n500\n1000"

## Execute shell in container

Open an interactive shell inside a running container.

```sh
podman exec -it <container> <shell>
```

$ shell: echo -e "/bin/bash\n/bin/sh" --- --prevent-extra

## Remove all stopped containers

Cleans up stopped containers to free disk space.
This does not affect running containers.

```sh
podman container prune -f
```

## Build image from Dockerfile

```sh
podman build -t <image_name>:<tag> <build_context>
```

$ build_context: echo -e ".\n.."
$ tag: echo -e "latest\ndev\ntest"

## Show disk usage

```sh
podman system df -v
```
