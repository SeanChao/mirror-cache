# Development Documentation

## Cache Policies

## `LruRedisCache`

TODO:

## `TtlRedisCache`

`TtlRedisCache` is a simple cache policy based on "time to live (TTL)". When an entry is cached, the entry is only valid within the given TTL. After TTL, the cache entry will be evicted.

The policy is implemented on top of redis command `EXPIRE`.
A cache hit happens if the program can `GET` cache key from redis. Otherwise a cache miss happens, and the program then `put` the cache entry.
A cache entry `put` is composed of two redis operations: `SET` the cache key and `EXPIRE` it.
