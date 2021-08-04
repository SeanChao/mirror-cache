#!/bin/zx
$.quote = v => v

import { pip_install, clean } from './lib.mjs'

const config = {
	'exec': 'cargo run',
	'mirror': 'http://localhost:9000/pypi/simple',
	...argv
}


let exitCode = 0;
try {
	await Promise.all([
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
}

await $`exit ${exitCode}`
