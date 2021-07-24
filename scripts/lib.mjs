export function pip_install(name, proxy) {
	return `docker \
	run \
	--rm \
	--network host \
	python \
	python -m \
	pip install \
	-i ${proxy} \
	--disable-pip-version-check \
	--retries 0 \
	--no-cache-dir \
	${name}`
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
