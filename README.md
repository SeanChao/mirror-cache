<h1 align="center">MirrorCache</h1>

<div align="center">
  <a href="https://github.com/SeanChao/mirror-cache/actions/workflows/test.yml">
    <img src="https://github.com/SeanChao/mirror-cache/actions/workflows/test.yml/badge.svg" alt="GitHub Action CI status"></img>
  </a>
</div>

A smart reverse proxy supporting multiple cache policies and customized rules, for mirror sites and also personal users!

## Quick start

First, start a [Redis](https://redis.io/) instance and update the connection string in config (`redis.url`).
For quick start, you may use this command to start a redis server in Docker: `make redis`.

Run the app:

```sh
cargo run
```

Try it out:

```sh
pip install -i http://localhost:9000 django
```

## More

We currently provide built-in support for [PyPI](https://pypi.org/) and [Anaconda](https://anaconda.com). You may add other regex-based rules dynamically.

See [docs](docs/README.md) for detailed documentation.
