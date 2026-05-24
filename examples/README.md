# nginx-vts-rust + Prometheus + Grafana example

A self-contained docker compose stack that builds `nginx` with the
`ngx_vts_rust` module, scrapes `/status` from Prometheus, and renders
the metrics in a pre-provisioned Grafana dashboard.

## Layout

```
examples/
├── docker-compose.yml
├── nginx/
│   ├── Dockerfile          # multi-stage: rust + nginx + slim runtime
│   └── nginx.conf          # vts_zone + proxy_cache_path + /status
├── prometheus/
│   └── prometheus.yml
└── grafana/
    ├── provisioning/       # datasource + dashboard loader
    └── dashboards/
        └── nginx-vts.json
```

The build context is the repository root, so the Dockerfile can `COPY`
the Rust crate. Run all commands from this `examples/` directory.

## Bring it up

```bash
cd examples
docker compose up --build
```

Initial build pulls Rust + nginx source and compiles both — expect
5–10 minutes the first time. Subsequent runs use the layer cache.

Once the stack is healthy:

| Service     | URL                              | Notes                  |
|-------------|----------------------------------|------------------------|
| nginx       | http://localhost:18080            | proxy + cache          |
| /status     | http://localhost:18080/status     | Prometheus text output |
| Prometheus  | http://localhost:9090            |                        |
| Grafana     | http://localhost:3000            | anonymous Viewer       |

The dashboard is under **Dashboards → nginx-vts → nginx-vts**.
Admin login is `admin` / `admin` if you need to edit.

## Drive some traffic

```bash
# Cacheable — first request is a MISS, the rest are HIT.
for i in $(seq 1 50); do curl -s http://localhost:18080/foo > /dev/null; done

# Force a BYPASS via the configured header.
for i in $(seq 1 10); do
  curl -s -H "X-Bypass: 1" http://localhost:18080/bypass/ > /dev/null
done

# Generate a steady-state load for ~30s (good for watching the dashboard tick).
for i in $(seq 1 600); do
  curl -s "http://localhost:18080/path-$((RANDOM % 5))" > /dev/null
  sleep 0.05
done
```

Then refresh the Grafana panel — the cache hit ratio gauge, the
stacked `cache_requests_total` chart, and the server/upstream request
rate panels should update.

## What's inside nginx.conf

- `vts_zone main 1m;` — 1 MB of shared memory for the cross-worker
  counter table.
- `proxy_cache_path … keys_zone=cache_zone1:1m` — the cache zone whose
  name becomes the `zone` label in `nginx_vts_cache_requests_total`.
- A `127.0.0.1:18091` origin server in the same container so the
  example needs no extra service.
- `location = /status { vts_status; access_log off; allow all; }` —
  Prometheus scrape target; `access_log off` keeps the scrape itself
  out of the access log, but note that the LOG_PHASE handler still
  increments `nginx_vts_server_requests_total{zone="example.test"}`
  for every scrape.

## Stop

```bash
docker compose down -v
```

`-v` also clears the Prometheus TSDB volume if you added one.
