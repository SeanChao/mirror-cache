# Documentation

## Configurations

An [example](../config.yml).

### How to configure

[config.yml](config.yml) is an example configuration file. Equivalent `config.{json|toml|ini}` files are also supported.
See [config-rs](https://github.com/mehcode/config-rs) for supported formats.

Exporting environment variables can override settings in the config file. All environment variables should be prefixed with `APP_`, Eg: `export APP_PORT=2333` overrides the `port` field to `2333`.

### Common Options

`port` specifies the port number to listen on.

`url` specifies the base URL for the application. It is used in upstream rewriting for some upstream like PyPI index pages.

`log_level` specifies the log level. Allowed values are `trace`, `debug`, `info`, `warn`, `error`.

### Redis

`url` is the Redis connection string.

### Builtin

In the builtin section, you can customize settings of the builtin rules.

We currently support the following mirror rules:

- `pypi_index`: PyPI index page
- `pypi_packages`: PyPI packages
- `anaconda_index`: Anaconda index page (repodata.json)
- `anaconda_packages`: Anaconda packages

You may configure the policy and upstream for these rules.

### Rules

Rules are an array of customized proxy rules.

- path: the path to match, supports regular expression
- policy: the name of policy to use, defined in `policies`
- upstream: the upstream of the path, the reverse proxy will try to fetch targets from the upstream

### Policies

Policies are an array of customized cache policies.

- name: the unique name of the policy
- type: the type of the policy, see [Cache Policies](#cache-policies) for details
- path: the path of cached data

## Cache Policies

### Lru Redis Cache

In config: `type: LRU`

`LruRedisCache` is a cache policy that limits the total **disk space usage**. All files are cached to the local filesystem.

If the size of all cached files exceeds specied limit, the program will evict a cache entry base on **least recent used (LRU)** policy.
Every time a file is accessed, its access time is updated to a newer one. Those cache entries with lease recent access time are evicted until we have enough space for the new cache entry.

### TTL Redis Cache

In config: `type: TTL`

> Note: To use this cache policy, please enable redis keyspace notifications and enable notifications for key expirations, that is: `notify-keyspace-events Kx`. See [redis.conf](../redis.conf) for reference.

`TtlRedisCache` is a simple cache policy based on "time to live (TTL)". When an entry is cached, the entry is only valid within the given TTL. After TTL, the cache entry will be evicted.

The policy is implemented on top of the redis command `EXPIRE`.
A cache hit happens if the program can `GET` cache key from redis. Otherwise a cache miss happens, and the program then `put` the cache entry.
A cache entry `put` is composed of two redis operations: `SET` the cache key and `EXPIRE` it.

If a key expiration notification is published while the program is not running, some cache data may not be removed from storage. You may need to manually remove them based on the TTL you configured.
