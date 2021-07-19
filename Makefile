build:
	cargo build

run:
	cargo run

dev:
	cargo watch -d 2 -i cache -x run

dev_clear:
	rm -rf cache/* 
	docker stop redis_dev || return 0
	docker run --name redis_dev -d --network host --rm redis
	cargo watch -d 2 -i cache -x run

dev_deps:
	cargo install cargo-watch
	yarn global add zx

redis:
	docker run --name redis_dev -d --network host --rm redis

redis_cli:
	docker run -it --network host --rm redis redis-cli

redis_dump:
	docker run -it --network host --rm redis redis-cli get total_size
	docker run -it --network host --rm redis redis-cli zrange cache_keys 0 -1 WITHSCORES

clean:
	rm -rf cache/* 
	docker stop redis_dev || return 0
