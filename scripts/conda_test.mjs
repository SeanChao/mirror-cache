#!/bin/zx
import { conda_install } from './lib.mjs'
$.quote = (v) => v

let exitCode = 0;
try {
	const mirror = 'http://localhost:9000/anaconda/pkgs/main'
	await $`${conda_install('django', mirror)}`

	// concurrent
	const targets = ["urllib3", "botocore", "six", "idna", "requests", "boto3", "certifi", "chardet", "setuptools", "python-dateutil", "s3transfer", "pyyaml", "pip", "typing-extensions"]
	await Promise.all(targets.map(e => $`${conda_install(e, mirror)}`));
} catch (p) {
	console.log(p);
	exitCode = p.exitCode;
}

await $`exit ${exitCode}`
