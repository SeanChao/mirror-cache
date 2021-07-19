#!/bin/zx
$.quote = v => v

function pip_install(name, proxy) {
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

function db_reset() {
	return `${redis_command('FLUSHALL')}`
}

function redis_command(cmd) {
	return `docker run --rm --network host redis redis-cli '${cmd}'`
}


function clean() {
	return db_reset();
}

const config = {
	'exec': 'cargo run',
	'mirror': 'http://localhost:9000/pypi/web/simple',
	...argv
}


const redis_container_name = 'redis_test_' + Math.floor(Math.random() * 1000);
let exitCode = 0;
try {
	await Promise.all([
		$`docker run --name ${redis_container_name} --network host --rm -d redis`,
		$`docker pull python`
	])
	// begin tests
	// test - pip, uncached
	await $`${clean()}`
	const pkg1 = '0==0.0.0'
	await $`${pip_install(pkg1, config.mirror)}`
	// test - pip, cached removed
	await $`rm -rf cache/8c/e6/83748ba1e232167de61f2bf31ec53f4b7acdd1ced52bdf3ea3366ea48132/0-0.0.0-py2.py3-none-any.whl` // size = 1992
	await $`${pip_install(pkg1, config.mirror)}`
	// test - pip, cached
	await $`${pip_install(pkg1, config.mirror)}`
	// end tests

	// begin tests
	const targets = ["urllib3", "botocore", "six", "idna", "requests", "boto3", "certifi", "chardet", "setuptools", "awscli", "python-dateutil", "s3transfer", "pyyaml", "pip", "typing-extensions"]
	await Promise.all(targets.map(e => $`${pip_install(e, config.mirror)}`));
	// end tests
} catch (p) {
	console.log(p);
	exitCode = p.exitCode;
} finally {
	console.log('Tests finished, cleaning up...')
	await $`docker stop ${redis_container_name}`
}

await $`exit ${exitCode}`
