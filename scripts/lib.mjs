export function pip_install(name, proxy) {
	return `docker \
	run \
	--rm \
	--network host \
	python \
	python -m \
	pip download \
	-i ${proxy} \
	--disable-pip-version-check \
	--no-cache-dir \
	${name}`
}

export function conda_install(name, proxy) {
	return `docker \
	run \
	--rm \
	--network host \
	continuumio/miniconda3 conda \
	install \
	--download-only \
	-v \
	-y \
	-c ${proxy} \
	${name}
	`
}

export function db_reset() {
	return `${redis_command('FLUSHALL')}`
}


export function clean() {
	return db_reset();
}

function redis_command(cmd) {
	return `docker run --rm --network host redis redis-cli '${cmd}'`
}
