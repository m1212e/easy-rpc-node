import { makeHTTPRequest } from "./request";

/**
 * Abstract class to initialize a remote rpc target. This abstract class helps performing requests and parsing responses from the corresponding server.
 */
export abstract class ERPCTarget {
  address: string;
  port: number;
  types: ("http-server" | "browser" | "tcp-server")[];

  constructor(
    adress: string,
    port: number,
    types: ("http-server" | "browser" | "tcp-server")[],
  ) {
    //TODO make compatibility request

    this.address = adress;
    this.port = port;
    this.types = types;
  }

  async call(methodIdentifier: string, parameters: any): Promise<any> {
    //TODO add checks for ensuring correct parameters, their types, array lengths, etc.
    if (this.types.find((e) => e == "http-server")) {
      return makeHTTPRequest(this.address, this.port, methodIdentifier, parameters);
    } else if (this.types.find((e) => e == "browser")) {
      throw new Error("Requests on browsers are not supported yet");
    } else if (this.types.find((e) => e == "tcp-server")) {
      throw new Error("Requests on tcp servers are not supported yet");
    } else {
      throw new Error("Unknown server type of target");
    }
  }
}
