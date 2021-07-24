# MirrorCache

## Getting Started

```sh
# run the server
make redis && make run
# clean up
make clean
```

## Configuration

If you want to use `TtlRedisCache`, please enable redis keyspace events notification, see [redis.conf](redis.conf) for reference.

## Development Documentation

See [docs](docs/README.md).
