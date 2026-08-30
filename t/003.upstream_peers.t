# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Per-peer upstream counters, ported from nginx-module-vts t/024.upstream_check.t
# and t/027.upstream_zone_peers.t. Those assert against the JSON display's
# upstreamZones; the equivalent here is a series labelled with both the
# upstream group and the peer address.

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 18;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: a proxied request is counted against its peer
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "peer"; }
    }

    upstream backend {
        server 127.0.0.1:1985;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Apeer\z/,
    qr/nginx_vts_upstream_requests_total\{upstream="backend",server="127\.0\.0\.1:1985"\} [1-9]\d*/,
]



=== TEST 2: upstream bytes are counted in both directions
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "peer"; }
    }

    upstream backend {
        server 127.0.0.1:1985;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Apeer\z/,
    qr/nginx_vts_upstream_bytes_total\{upstream="backend",server="127\.0\.0\.1:1985",direction="in"\} [1-9]\d*/,
]



=== TEST 3: every peer of the group gets its own series
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "a"; }
    }
    server {
        listen 1986;
        location / { return 200 "b"; }
    }

    upstream backend {
        server 127.0.0.1:1985;
        server 127.0.0.1:1986;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\A[ab]\z/,
    qr/\A[ab]\z/,
    qr/server="127\.0\.0\.1:1985".*\n(.*\n)*.*server="127\.0\.0\.1:1986"/,
]



=== TEST 4: upstream status classes are recorded
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 503 "unavailable"; }
    }

    upstream backend {
        server 127.0.0.1:1985;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /status']
--- error_code eval
[503, 200]
--- response_body_like eval
[
    qr/unavailable/,
    qr/nginx_vts_upstream_responses_total\{upstream="backend",server="127\.0\.0\.1:1985",status="5xx"\} [1-9]\d*/,
]
