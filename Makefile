REDIS_OPTS=-v ${PWD}/redis.conf:/conf/redis.conf --rm

build:
	cargo build

run:
	cargo run

test:
	docker stop redis_test || return 0
	docker run $(REDIS_OPTS) --name redis_test -d -p 3001:6379 --rm redis /conf/redis.conf
	rm -r cache || return 0
	RUST_BACKTRACE=1 cargo test $(ARGS)
	docker stop redis_test

dev:
	cargo watch -d 2 -w src -w Cargo.toml -x "run -- -c config.yml"

dev_clear:
	rm -rf cache/* || return 0
	docker stop redis_dev || return 0
	docker run --name redis_dev -d --network host --rm redis
	cargo watch -d 2 -i cache -x run

dev_deps:
	cargo install cargo-watch
	yarn global add zx

redis:
	docker run $(REDIS_OPTS) --name redis_dev -d --network host --rm redis /conf/redis.conf

redis_stop:
	docker stop redis_dev

redis_test:
	docker stop redis_test || return 0
	docker run $(REDIS_OPTS) --name redis_test -d -p 3001:6379 --rm redis /conf/redis.conf

redis_cli:
	docker run -it --network host --rm redis redis-cli

test_redis_cli:
	docker run -it --network host --rm redis redis-cli -p 3001

redis_dump:
	docker run -it --network host --rm redis redis-cli get total_size
	docker run -it --network host --rm redis redis-cli zrange cache_keys 0 -1 WITHSCORES

test_redis_dump:
	docker run -it --network host --rm redis redis-cli -p 3001 get total_size
	docker run -it --network host --rm redis redis-cli -p 3001 zrange cache_keys 0 -1 WITHSCORES

clean:
	rm -rf cache/* 
	docker stop redis_dev || return 0
	docker stop redis_test || return 0

scenario_test:
	zx ./scripts/pip_test.mjs
	zx ./scripts/conda_test.mjs
	zx ./scripts/conda_cloud_test.mjs
	zx ./scripts/stress.mjs

metrics:
	docker run -d --rm --network host -v $$PWD/prom.yml:/srv/prom.yml:ro --name metrics prom/prometheus --config.file /srv/prom.yml

metrics_stop:
	docker stop metrics
