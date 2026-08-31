#!/usr/bin/env node
// Differential-test oracle: reads text on stdin, runs it through the
// vendored legacy compress.js (not a reimplementation), prints
// {"compressed": "..."} on stdout.
const path = require('path');
const { compress } = require(
  path.join(__dirname, 'caveman-shrink-compress.js')
);

let input = '';
process.stdin.on('data', (c) => { input += c; });
process.stdin.on('end', () => {
  const { compressed } = compress(input);
  process.stdout.write(JSON.stringify({ compressed }));
});
