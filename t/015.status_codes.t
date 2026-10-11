# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Per-status-code counters, vts_measure_status_codes, the counterpart of the
# original's vhost_traffic_status_measure_status_codes.
#
# The original suite never turns that directive on, so nothing there to port:
# these are written against what its display prints. A listed code gets a row
# per zone even at zero; under `all` only the codes that occurred do; every
# zone gets an `other` row for what is not measured, and the host="*" row
# sums the zones.

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 38;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: listed codes are counted, and the rest goes to other
--- http_config
    vts_zone main 1m;
    vts_measure_status_codes 200 404;
--- config
    location /ok       { return 200 "ok"; }
    location /notfound { return 404 "nope"; }
    location /broken   { return 500 "broken"; }
    location /status   { vts_status; }
--- request eval
['GET /ok', 'GET /ok', 'GET /notfound', 'GET /broken', 'GET /status']
--- error_code eval
[200, 200, 404, 500, 200]
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/\Aok\z/,
    qr/nope/,
    qr/broken/,
    qr/# TYPE nginx_vts_status_code_requests_total counter\n(?s:.*)nginx_vts_status_code_requests_total\{host="localhost",code="200"\} 2\nnginx_vts_status_code_requests_total\{host="localhost",code="404"\} 1\nnginx_vts_status_code_requests_total\{host="localhost",code="other"\} 1\n/,
]



=== TEST 2: a listed code that never occurred still reads zero
--- http_config
    vts_zone main 1m;
    vts_measure_status_codes 200 503;
--- config
    location /ok     { return 200 "ok"; }
    location /status { vts_status; }
--- request eval
['GET /ok', 'GET /status']
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/nginx_vts_status_code_requests_total\{host="localhost",code="503"\} 0\n/,
]



=== TEST 3: all counts every code that occurred, and only those
--- http_config
    vts_zone main 1m;
    vts_measure_status_codes all;
--- config
    location /ok     { return 200 "ok"; }
    location /teapot { return 418 "teapot"; }
    location /status { vts_status; }
--- request eval
['GET /ok', 'GET /teapot', 'GET /status']
--- error_code eval
[200, 418, 200]
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/teapot/,
    qr/\A(?!.*code="404")(?s:.*)nginx_vts_status_code_requests_total\{host="localhost",code="200"\} 1\nnginx_vts_status_code_requests_total\{host="localhost",code="418"\} 1\nnginx_vts_status_code_requests_total\{host="localhost",code="other"\} 0\n/,
]



=== TEST 4: the host="*" row sums the codes of every zone
--- http_config
    vts_zone main 1m;
    vts_measure_status_codes all;
    server {
        listen 1985;
        server_name other.test;
        location / { return 204; }
    }
--- config
    location /ok     { return 200 "ok"; }
    location /proxy  { proxy_pass http://127.0.0.1:1985/; }
    location /status { vts_status; }
--- request eval
['GET /ok', 'GET /proxy', 'GET /status']
--- error_code eval
[200, 204, 200]
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/\A\z/,
    qr/code="204"\} 1\n(?s:.*)\{host="\*",code="200"\} 1\nnginx_vts_status_code_requests_total\{host="\*",code="204"\} 2\n/,
]



=== TEST 5: the other.test zone has its own rows
--- http_config
    vts_zone main 1m;
    vts_measure_status_codes all;
    server {
        listen 1985;
        server_name other.test;
        location / { return 204; }
    }
--- config
    location /proxy  { proxy_pass http://127.0.0.1:1985/; }
    location /status { vts_status; }
--- request eval
['GET /proxy', 'GET /status']
--- error_code eval
[204, 200]
--- response_body_like eval
[
    qr/\A\z/,
    qr/nginx_vts_status_code_requests_total\{host="other.test",code="204"\} 1\nnginx_vts_status_code_requests_total\{host="other.test",code="other"\} 0\n/,
]



=== TEST 6: without the directive there is no such family
--- http_config
    vts_zone main 1m;
--- config
    location /ok     { return 200 "ok"; }
    location /status { vts_status; }
--- request eval
['GET /ok', 'GET /status']
--- response_body_unlike eval
[
    qr/status_code_requests_total/,
    qr/status_code_requests_total/,
]



=== TEST 7: a code outside 100-599 is refused
--- http_config
    vts_zone main 1m;
    vts_measure_status_codes 200 600;
--- config
    location /status { vts_status; }
--- must_die
--- error_log
vts_measure_status_codes: invalid status code "600"



=== TEST 8: all does not mix with codes
--- http_config
    vts_zone main 1m;
    vts_measure_status_codes all 200;
--- config
    location /status { vts_status; }
--- must_die
--- error_log
vts_measure_status_codes: "all" cannot be combined with status codes
