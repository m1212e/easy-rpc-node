import { Server, createServer } from "http";
//TODO write all communication stuff into the specifications (e.g. how a method identifier works)

/**
 * Abstract class to initialize a rpc server.
 */
export abstract class ERPCServer {
  port: number;
  types: ("http-server" | "browser" | "tcp-server")[];
  private mappedCallbacks = {};
  private serverInstance: Server;

  constructor(
    port: number,
    types: ("http-server" | "browser" | "tcp-server")[]
  ) {
    //TODO make compatibility request

    this.port = port;
    this.types = types;

    //TODO correct type check
    if (types.length == 0) {
      throw new Error("Types cant be empty");
    }

    this.serverInstance = createServer(async (req, res) => {
      if (req.url == undefined) {
        res.writeHead(400);
        res.end();
        return;
      }

      const body = await new Promise<any[]>((resolve) => {
        let data = "";
        req.on("data", (chunk) => {
          data += chunk;
        });
        req.on("end", () => {
          resolve(JSON.parse(data));
        });
      });

      //TODO what if not found
      const result = await this.mappedCallbacks[req.url](...body);
      res.end(JSON.stringify(result));
    });
  }

  /**
    This method is used by easy-rpc internally and is not intended for manual use. It can be used to register a function on the server dynamically.
  */
  registerERPCCallbackFunction(func, identifier) {
    this.mappedCallbacks[identifier] = func;
  }

  /**
   * Returns a promise which resolves when the server has been started
   */
  start() {
    return new Promise<void>((resolve) => {
      this.serverInstance.listen(this.port, "localhost", () => {
        console.log(`easy-rpc http server listening on localhost:${this.port}`);
        resolve();
      });
    });
  }

  /**
   * Returns a promise which resolves when the server has been stopped
   */
  stop() {
    return new Promise<Error | undefined>((resolve, reject) => {
      this.serverInstance.close((err) => {
        if (!err) {
          reject(err);
        } else {
          resolve(undefined);
        }
      });
    });
  }
}
