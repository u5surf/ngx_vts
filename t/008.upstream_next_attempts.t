# vi:set ft=perl ts=4 sw=4 et fdm=marker:

# Ported from nginx-module-vts t/045.upstream_next_attempts.t.
#
# The original module recorded u->state, the state of the last attempt, so a
# request that proxy_next_upstream passed on recorded nothing at all against
# the peer it was passed on from - the peer an operator looks for first. This
# module walks r->upstream_states instead, which should record every attempt.
#
# The upstream has a primary with nothing listening on it and a backup that
# answers, so the order of attempts is the same for every request. max_fails=0
# keeps the primary in play rather than having it marked down after the first
# failure.
#
# What each peer is expected to hold:
#
#   primary   a request count and a 5xx, no bytes, since it served no client
#   backup    a request count, a 2xx, and the bytes

use Test::Nginx::Socket;

repeat_each(1);
plan tests => 30;
no_shuffle();
run_tests();

__DATA__

=== TEST 1: the peer that was passed on is counted at all
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "backup"; }
    }

    upstream backend {
        server 127.0.0.1:1981 max_fails=0;
        server 127.0.0.1:1985 backup;
    }
--- config
    location /up {
        proxy_next_upstream error timeout;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Abackup\z/,
    qr/\Abackup\z/,
    qr/nginx_vts_upstream_requests_total\{upstream="backend",server="127\.0\.0\.1:1981"\} 2\b/,
]

# The series is written whether or not anything was counted, so asserting that
# the address appears would pass even if the attempt were dropped. The counter
# is what says the attempt was seen.



=== TEST 2: it holds the status of its own attempt, not the client's
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "backup"; }
    }

    upstream backend {
        server 127.0.0.1:1981 max_fails=0;
        server 127.0.0.1:1985 backup;
    }
--- config
    location /up {
        proxy_next_upstream error timeout;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Abackup\z/,
    qr/\Abackup\z/,
    qr/nginx_vts_upstream_responses_total\{upstream="backend",server="127\.0\.0\.1:1981",status="5xx"\} 2\b/,
]



=== TEST 3: the peer that was passed on moved no bytes
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "backup"; }
    }

    upstream backend {
        server 127.0.0.1:1981 max_fails=0;
        server 127.0.0.1:1985 backup;
    }
--- config
    location /up {
        proxy_next_upstream error timeout;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Abackup\z/,
    qr/\Abackup\z/,
    qr/nginx_vts_upstream_bytes_total\{upstream="backend",server="127\.0\.0\.1:1981",direction="in"\} 0\b/,
]



=== TEST 4: the peer that answered is counted as it always was
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "backup"; }
    }

    upstream backend {
        server 127.0.0.1:1981 max_fails=0;
        server 127.0.0.1:1985 backup;
    }
--- config
    location /up {
        proxy_next_upstream error timeout;
        proxy_pass http://backend/;
    }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Abackup\z/,
    qr/\Abackup\z/,
    qr/nginx_vts_upstream_responses_total\{upstream="backend",server="127\.0\.0\.1:1985",status="2xx"\} 2\b/,
]



=== TEST 5: a request that needs no second attempt is unchanged
--- http_config
    vts_zone main 1m;

    server {
        listen 1985;
        location / { return 200 "only"; }
    }

    upstream backend {
        server 127.0.0.1:1985;
    }
--- config
    location /up     { proxy_pass http://backend/; }
    location /status { vts_status; }
--- request eval
['GET /up', 'GET /up', 'GET /status']
--- response_body_like eval
[
    qr/\Aonly\z/,
    qr/\Aonly\z/,
    qr/nginx_vts_upstream_requests_total\{upstream="backend",server="127\.0\.0\.1:1985"\} 2\b/,
]
