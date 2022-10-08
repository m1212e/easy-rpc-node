import test from 'ava'
import {ERPCServer} from '../index.js'

test('create erpc server', (t) => {

  const s = new ERPCServer({
    port: 9988,
    allowedCorsOrigins: ["*"]
  }, true, "role");
})
