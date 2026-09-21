# ssh

## Login to server with key

```sh
ssh -i <sshkey> -p <port> <user>@<server>
```

$ user: echo -e "$(whoami)\nroot" --- --prevent-extra

## Show connected devices

```bash
arp -a
```

# podman, containers

## List running containers

```
podman ps
```

## Stop a container

```sh
podman stop <container_id>
```

$ container_id: podman ps --format '{{.ID}}\t{{.Names}}' --- --column 1
