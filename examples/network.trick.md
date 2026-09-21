# network

## Ping a host

Send ICMP echo requests to test connectivity.
Use Ctrl-C to stop.

```sh
ping -c <count> <host>
```

$ host: echo -e "8.8.8.8\n1.1.1.1\nlocalhost"
$ count: echo -e "4\n10\n100"

## Check open ports

Scan common ports on a remote host using nmap.

```sh
nmap -sT --top-ports 20 <host>
```

## DNS lookup

```sh
dig <domain> <record_type>
```

$ domain: echo -e "google.com\nexample.com\ngithub.com"
$ record_type: echo -e "A\nAAAA\nMX\nNS\nTXT\nCNAME" --- --prevent-extra

## Show network interfaces

```sh
ip -br -c addr show
```

## Trace route to host

Shows the path packets take to reach a destination.
Each hop is a router between you and the target.

```sh
traceroute <host>
```

## Check listening ports

```sh
ss -tlnp
```
