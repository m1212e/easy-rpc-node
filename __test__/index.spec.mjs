import test from 'ava'
import {ERPCServer, ERPCTarget} from '../index.js'

//TODO test sockets

test('test creation', (t) => {

  const server = new ERPCServer({
    port: 9988,
    allowedCorsOrigins: ["*"]
  }, "http-server", true, "Backend");

  const target = new ERPCTarget({
    address: "",
    port: 0
  }, "http-server")

  t.assert(server != undefined)
  t.assert(target != undefined)
})

test('test handler calls', async (t) => {

  const server = new ERPCServer({
    port: 9988,
    allowedCorsOrigins: ["*"]
  }, "http-server", true, "Backend");

  server.registerERPCHandler((p1, p2) => {
    t.assert(p1 == "p1")
    t.assert(p2 == 17)

    return "helllloooo"
  }, "some/handler/identifier")

  server.registerERPCHandler((p1) => {
    t.assert(p1 == undefined)
  }, "some/handler/identifier/two")


  setTimeout(() => {
    server.stop();
  }, 5000);

  const target = new ERPCTarget({
    address: "http://localhost",
    port: 9988
  }, "http-server")

  setTimeout(async () => {
    let r = await target.call("some/handler/identifier", ["p1", 17])
    t.assert(r == "helllloooo")

    let r2 = await target.call("some/handler/identifier/two")
    t.assert(r2 == undefined)
  }, 1000);

  await server.run();
})