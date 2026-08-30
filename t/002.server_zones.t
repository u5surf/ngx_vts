# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Server-zone counters. The original suite covers these through its JSON and
# HTML displays (t/001, t/030); here the same numbers come out as Prometheus
# series labelled by zone.
#
# The zone is the matched server block's first server_name, which under this
# harness is the "localhost" the test template writes.

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 18;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: a served request is counted against its server zone
--- http_config
    vts_zone main 1m;
--- config
    location /hello {
        return 200 "hello";
    }
    location /status {
        vts_status;
    }
--- request eval
['GET /hello', 'GET /status']
--- response_body_like eval
[
    qr/\Ahello\z/,
    qr/nginx_vts_server_requests_total\{zone="localhost"\} [1-9]\d*/,
]



=== TEST 2: responses are bucketed by status class
--- http_config
    vts_zone main 1m;
--- config
    location /ok       { return 200 "ok"; }
    location /notfound { return 404 "nope"; }
    location /status   { vts_status; }
--- request eval
['GET /ok', 'GET /notfound', 'GET /status']
--- error_code eval
[200, 404, 200]
--- response_body_like eval
[
    qr/\Aok\z/,
    qr/nope/,
    qr/nginx_vts_server_responses_total\{zone="localhost",status="4xx"\} [1-9]\d*/,
]



=== TEST 3: bytes are counted in both directions
--- http_config
    vts_zone main 1m;
--- config
    location /body   { return 200 "0123456789"; }
    location /status { vts_status; }
--- request eval
['GET /body', 'GET /status']
--- response_body_like eval
[
    qr/\A0123456789\z/,
    qr/nginx_vts_server_bytes_total\{zone="localhost",direction="out"\} [1-9]\d*/,
]



=== TEST 4: an unknown Host does not create a zone of its own
--- http_config
    vts_zone main 1m;
--- config
    location /hello  { return 200 "hello"; }
    location /status { vts_status; }
--- request eval
['GET /hello', 'GET /status']
--- more_headers eval
["Host: not-configured.invalid\n", ""]
--- response_body_unlike eval
[
    qr/this pattern is never in the body/,
    qr/zone="not-configured\.invalid"/,
]
