# Documentation

## Configurations

The example [configuration file](../config.yml).

### How to configure

[config.yml](config.yml) is an example configuration file. Equivalent `config.{json|toml|ini}` files are also supported.
See [config-rs](https://github.com/mehcode/config-rs) for supported formats.

Exporting environment variables can override settings in the config file. All environment variables should be prefixed with `APP_`, Eg: `export APP_PORT=2333` overrides the `port` field to `2333`.

#### Data type

The type of `size` in the config file is string. E.g: `1000` (B), `42 KB`, `2.33 MB`, `666 GiB`.

#### Common Options

`port` specifies the port number to listen on.

`metrics_port`: specifies the port of Prometheus metrics server.

`url` specifies the base URL for the application. It is used in upstream rewriting for some upstream like PyPI index pages.

`log_level` specifies the log level. Allowed values are `trace`, `debug`, `info`, `warn`, `error`.

#### Redis

`url` is the Redis connection string.

#### Rules

Rules are an array of customized proxy rules.

- `path`: the path to match, supports regular expression. If the given string is a plain string, a simple prefix removal and reverse proxying is performed: the target url is the content after `path` appended to the `upstream`.
- `policy`: the name of policy to use, defined in `policies`
- `upstream`: the upstream of the path, the reverse proxy will try to fetch targets from the upstream
- `size_limit`: *Optional* The maximum size of package that the program would fetch and cache. If the size of the package exceeds the number, the response will be a `302 Found` to the upstream url. Use `0` for unlimited size. The default value is `0`.
- `options`: *Optional* Additional options for the rule.
  - `content-type`: Override the content-type of the response. Some endpoints like PyPI index requires this header.

#### Policies

Policies are an array of customized cache policies.

- name: the unique name of the policy
- type: the type of the policy, see [Cache Policies](#cache-policies) for details
- path: the path of cached data

### Hot reloading

Any changes on `config.yml` will trigger a configuration reload after a delay of 2 secs.

Note that some configurations like `port` and `log_level` cannot be updated.

## Cache Policies

### Lru Redis Cache

In config: `type: LRU`

`LruRedisCache` is a cache policy that limits the total **disk space usage**. All files are cached to the local filesystem.

If the size of all cached files exceeds specied limit, the program will evict a cache entry base on **least recent used (LRU)** policy.
Every time a file is accessed, its access time is updated to a newer one. Those cache entries with lease recent access time are evicted until we have enough space for the new cache entry.

Avaliable options in `policy`:
- `size`: the maximum size of the space usage.

### TTL Redis Cache

In config: `type: TTL`

> Note: To use this cache policy, please enable redis keyspace notifications and enable notifications for key expirations, that is: `notify-keyspace-events Kx`. See [redis.conf](../redis.conf) for reference.

`TtlRedisCache` is a simple cache policy based on "time to live (TTL)". When an entry is cached, the entry is only valid within the given TTL. After TTL, the cache entry will be evicted.

The policy is implemented on top of the redis command `EXPIRE`.
A cache hit happens if the program can `GET` cache key from redis. Otherwise a cache miss happens, and the program then `put` the cache entry.
A cache entry `put` is composed of two redis operations: `SET` the cache key and `EXPIRE` it.

If a key expiration notification is published while the program is not running, some cache data may not be removed from storage. You may need to manually remove them based on the TTL you configured.

Avaliable options in `policy`:
- timeout: The TTL in seconds.

## Metrics

The prometheus metrics server is exposed on the specified port in config. You may launch a prometheus client and configure the target with the port.
