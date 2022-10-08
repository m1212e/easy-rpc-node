const {ErpcServer} = require("./index")

const PORT = 8899;

let t = new ErpcServer({
    port: PORT,
    allowedCorsOrigins: ["hello"]
}, ["a"], false, "role")

// t.registerERPCCallbackFunction((username, password) => {
//     return new Promise((resolve) => {
//         resolve({result: "hello from the result2"});
//     })
// }, "/login")

t.registerERPCCallbackFunction(async (username, password) => {
    return {result: "never resolves"};
}, "/async")

t.registerERPCCallbackFunction((username, password) => {
    return {result: "resolves"};
}, "/sync")

t.run()

/*
    Now make a http request to "localhost:8899/sync" or "localhost:8899/async".
    sync should return instantly, async should never resolve

    The relevant code is in src/lib.rs line 66
    The src/threadsafe_function.rs originates here: https://github.com/napi-rs/napi-rs/issues/1307
*/