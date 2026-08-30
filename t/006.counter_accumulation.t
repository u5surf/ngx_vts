# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Counters accumulate across requests, ported from the half of
# nginx-module-vts t/042.dump.t that does not depend on the dump directive.
#
# The original interleaves requests and displays and walks the counter up
# through 1, then 2-9, then 3-9, which catches a display that reports a single
# request's numbers rather than the running total. The floors are ranges rather
# than exact numbers there, and have to be here too: the status handler runs
# the log phase like any other location, so each display is itself counted.
#
# The rest of that file restarts nginx and checks the counters survived, which
# needs vhost_traffic_status_dump; there is no equivalent here.

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 36;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: the request counter walks up rather than resetting
--- http_config
    vts_zone main 1m;
--- config
    location /hello  { return 200 "hello"; }
    location /status { vts_status; }
--- request eval
['GET /hello', 'GET /status', 'GET /hello', 'GET /status', 'GET /hello', 'GET /status']
--- response_body_like eval
[
    qr/\Ahello\z/,
    qr/nginx_vts_server_requests_total\{zone="localhost"\} [1-9]\d*/,
    qr/\Ahello\z/,
    qr/nginx_vts_server_requests_total\{zone="localhost"\} [2-9]\d*/,
    qr/\Ahello\z/,
    qr/nginx_vts_server_requests_total\{zone="localhost"\} [3-9]\d*/,
]



=== TEST 2: status classes accumulate independently
--- http_config
    vts_zone main 1m;
--- config
    location /ok       { return 200 "ok"; }
    location /notfound { return 404 "nope"; }
    location /status   { vts_status; }
--- request eval
['GET /ok', 'GET /notfound', 'GET /notfound', 'GET /status']
--- error_code eval
[200, 404, 404, 200]
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/nope/,
    qr/nope/,
    qr/nginx_vts_server_responses_total\{zone="localhost",status="4xx"\} 2\b/,
]



=== TEST 3: bytes accumulate rather than reporting the last response
--- http_config
    vts_zone main 1m;
--- config
    location /body   { return 200 "0123456789"; }
    location /status { vts_status; }
--- request eval
['GET /body', 'GET /status', 'GET /body', 'GET /status']
--- response_body_like eval
[
    qr/\A0123456789\z/,
    qr/nginx_vts_server_bytes_total\{zone="localhost",direction="out"\} (\d+)/,
    qr/\A0123456789\z/,
    qr/nginx_vts_server_bytes_total\{zone="localhost",direction="out"\} [1-9]\d{2,}/,
]



=== TEST 4: upstream counters accumulate per peer
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "peer"; }
    }
    upstream backend { server 127.0.0.1:1985; }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /up', 'GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Apeer\z/,
    qr/\Apeer\z/,
    qr/\Apeer\z/,
    qr/nginx_vts_upstream_requests_total\{upstream="backend",server="127\.0\.0\.1:1985"\} 3\b/,
]
