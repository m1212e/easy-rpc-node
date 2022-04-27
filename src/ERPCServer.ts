import { WebSocket, WebSocketServer } from "ws";
import { Server, createServer, IncomingMessage, ServerResponse } from "http";
import { ServerOptions } from "./Options";
//TODO write all communication stuff into the specifications (e.g. how a method identifier works)

/**
 * Abstract class to initialize a rpc server.
 */
export abstract class ERPCServer {
  readonly options: ServerOptions;
  readonly types: ("http-server" | "browser")[];
  readonly enableSockets: boolean;
  readonly role: string;

  private connectedSockets: { role: string; client: WebSocket }[] = [];
  private connectionCallbacks: (({
    role: string,
    client: WebSocket,
  }) => void)[] = [];
  private mappedCallbacks = {};
  private serverInstance: Server;
  private socketServerInstance: WebSocketServer;

  constructor(
    options: ServerOptions,
    types: ("http-server" | "browser")[],
    enableSockets: boolean,
    role: string
  ) {
    //TODO make compatibility request

    this.types = types;
    this.options = options;
    this.enableSockets = enableSockets;
    this.role = role;

    //TODO correct type check
    if (types.length == 0) {
      throw new Error("Types cant be empty");
    }

    this.serverInstance = createServer(async (req, res) => {
      //TODO CORS

      if (req.method == "OPTIONS") {
        this.setCorsHeader(req, res);
        res.writeHead(200);
        res.end();
        return;
      }

      if (req.url == undefined || req.method != "POST") {
        res.writeHead(400);
        res.end();
        return;
      }

      this.setCorsHeader(req, res);

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
    const httpServer = new Promise<void>((resolve) => {
      this.serverInstance.listen(this.options.port, "localhost", () => {
        process.stdout.write("🚀 ");
        console.log(
          "\x1b[36m%s\x1b[0m",
          `easy-rpc http server listening on localhost:${this.options.port}`
        );
        resolve();
      });
    });

    const socketServer = new Promise<void>((resolve) => {
      if (this.enableSockets) {
        this.socketServerInstance = new WebSocketServer({
          server: this.serverInstance,
        });

        this.socketServerInstance.on("error", (err) => {
          console.error(err.message);
        });

        this.socketServerInstance.on("connection", (client) => {
          const handle = ({ data }) => {
            const parsedData = JSON.parse(data);
            if (parsedData.role) {
              client.removeEventListener("message", handle);
              const o = { role: parsedData.role, client };
              this.connectedSockets.push(o);
              this.connectionCallbacks.forEach((f) => f(o));
            } else {
              console.error("Could not read role from connected client:", data);
            }
          };
          client.addEventListener("message", handle);
        });

        this.socketServerInstance.on("listening", () => {
          resolve();
        });
      } else {
        resolve();
      }
    });

    return Promise.all([httpServer, socketServer]);
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

  onSocketConnection(callback: ({ role: string, client: WebSocket }) => void) {
    this.connectionCallbacks.push(callback);
    this.connectedSockets.forEach((s) => callback(s));
  }

  private setCorsHeader(req: IncomingMessage, res: ServerResponse) {
    if (req.headers.origin) {
      for (const allowedOrigin of this.options.allowedCorsOrigins) {
        if (req.headers.origin == allowedOrigin) {
          res.setHeader("Access-Control-Allow-Origin", allowedOrigin);
          res.setHeader("Access-Control-Allow-Credentials", "true");
          break;
        }
      }
    }
  }
}
