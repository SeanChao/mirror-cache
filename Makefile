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
k
clean:
	rm -rf cache/* 
	docker stop redis_dev || return 0
