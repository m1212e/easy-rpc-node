import test from 'ava'
import {ERPCServer, ERPCTarget} from '../index.js'

test('create erpc server', (t) => {

  new ERPCServer({
    port: 9988,
    allowedCorsOrigins: ["*"]
  }, true, "role");

  new ERPCTarget({
    address: "",
    port: 0
  }, ["http-server"])

})
