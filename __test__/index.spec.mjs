import test from 'ava'

import { sum } from '../index.js'

test('sum from native', (t) => {
  t.timeout(1000 * 60 * 10);
  t.is(sum(1, 2), 3)
})
