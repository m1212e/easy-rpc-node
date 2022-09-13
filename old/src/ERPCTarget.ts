import { WebSocket } from "ws";
import { TargetOptions } from "./Options";
import { makeHTTPRequest } from "./request";

//TODO do all sorts of error handling
//TODO request timeout

/**
 * Abstract class to initialize a remote rpc target. This abstract class helps performing requests and parsing responses from the corresponding server.
 */
export abstract class ERPCTarget {
  private options: TargetOptions;
  private types: ("http-server" | "browser")[];
  private socket: WebSocket;
  private requestID = 0;
  private requests: {
    id: number;
    resolve: (t: unknown) => void;
    reject: (t: unknown) => void;
  }[] = [];

  constructor(
    options: TargetOptions,
    types: ("http-server" | "browser")[],
  ) {
    //TODO make compatibility request

    this.options = options;
    this.types = types;
  }

  private async call(methodIdentifier: string, parameters: any): Promise<any> {
    //TODO add checks for ensuring correct parameters, their types, array lengths, etc.
    if (this.types.find((e) => e == "http-server")) {
      return makeHTTPRequest(this.options, methodIdentifier, parameters);
    } else if (this.types.find((e) => e == "browser")) {
      return new Promise((resolve, reject) => {
        this.requests.push({ id: this.requestID, resolve, reject });
        this.socket.send(
          JSON.stringify({
            id: this.requestID++,
            body: parameters,
            url: methodIdentifier,
          })
        );
      });
    } else {
      throw new Error("Unknown server type of target");
    }
  }

  /**
   * This method is used by ERPC internally and is not meant for manual use. It sets the socket object of this target, which is the used to process requests to a webbrowser.
   * @param socket The socket object to set
   */
  private setERPCSocket(socket: WebSocket) {
    this.socket = socket;
    socket.addEventListener("message", ({ data }) => {
      const parsedData = JSON.parse(data.toString());
      const i = this.requests.findIndex((e) => e.id == parsedData.id);
      this.requests[i].resolve(parsedData.body);
      this.requests.splice(i, 1);
    });
  }
}
