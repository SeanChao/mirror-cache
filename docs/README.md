# Documentation

## Configurations

An [example](../config.yml)

### Common Options

### Rules

Rules are an array of customized proxy rules.

- path: the path to match, supports regular expression
- policy: the name of policy to use, defined in `policies`
- upstream: the upstream of the path, the reverse proxy will try to fetch targets from the upstream

## Cache Policies

## Lru Redis Cache

Config key: `LRU`

`LruRedisCache` is a cache policy that limits the total **disk space usage**. All files are cached to the local filesystem.

If the size of all cached files exceeds specied limit, the program will evict a cache entry base on **least recent used (LRU)** policy.
Every time a file is accessed, its access time is updated to a newer one. Those cache entries with lease recent access time are evicted until we have enough space for the new cache entry.

## TTL Redis Cache

Config key: `TTL`

> Note: To use this cache policy, please enable redis keyspace events notification, see [redis.conf](../redis.conf) for reference.

`TtlRedisCache` is a simple cache policy based on "time to live (TTL)". When an entry is cached, the entry is only valid within the given TTL. After TTL, the cache entry will be evicted.

The policy is implemented on top of the redis command `EXPIRE`.
A cache hit happens if the program can `GET` cache key from redis. Otherwise a cache miss happens, and the program then `put` the cache entry.
A cache entry `put` is composed of two redis operations: `SET` the cache key and `EXPIRE` it.
