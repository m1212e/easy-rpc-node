const index = require("./index")

index.ErpcServer.registerERPCCallbackFunction = function(func, identifier) {
    return index.ErpcServer.registerERPCCallbackFunction(async (args, finishCallback) => {
        const result = await func(...args);
        finishCallback(result);
    }, identifier)
}

module.export = index;