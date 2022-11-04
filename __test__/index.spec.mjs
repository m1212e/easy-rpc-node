import test from 'ava'
import {ERPCServer, ERPCTarget} from '../index.js'

test('create erpc server', (t) => {

  const server = new ERPCServer({
    port: 9988,
    allowedCorsOrigins: ["*"]
  }, ["http-server"], true, "Backend");

  const target = new ERPCTarget({
    address: "",
    port: 0
  }, ["http-server"])

  t.assert(server != undefined)
  t.assert(target != undefined)
})
