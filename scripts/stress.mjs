#!/bin/zx
$.quote = v => v

import { pip_install } from './lib.mjs'

const config = {
	'mirror': 'http://localhost:9000/pypi/simple',
	...argv
}

let exitCode = 0;
try {
	// begin tests
	const base = ["torch", "tensorflow"]
	let targets = []
	for (let i = 0; i < 5; i++) {
		targets = targets.concat(base)
	}
	console.log(targets)
	await Promise.all(targets.map(e => $`${pip_install(e, config.mirror)}`));
	// end tests
} catch (p) {
	console.log(p);
	exitCode = p.exitCode;
} finally {
	console.log('Tests finished, cleaning up...')
}

await $`exit ${exitCode}`
