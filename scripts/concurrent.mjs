#!/bin/zx
$.quote = v => v

import { pip_install } from './lib.mjs'

const reqs = []
for (let i = 0; i < 20; i++) {
	reqs.push($`${pip_install('django', 'http://localhost:9000/pypi/web/simple')}`);
}
await Promise.all(reqs)
