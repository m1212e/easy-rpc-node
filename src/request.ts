import { request as secureRequest } from "https";
import { ClientRequest, request } from "http";
import { TargetOptions } from "./Options";

export async function makeHTTPRequest(
  options: TargetOptions,
  methodIdentifier: string,
  parameters?: any
): Promise<any> {
  const body = JSON.stringify(parameters);

  const requestOptions = {
    hostname: options.address,
    port: options.port,
    path: methodIdentifier,
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Content-Length": body.length,
    },
  };

  return new Promise((resolve, reject) => {
    const handler = (res) => {
      if (res.statusCode != 200) {
        reject(`Recieved status code ${res.statusCode} instead of 200`);
        return;
      }

      let data = "";

      res.on("data", (d) => {
        data += d;
      });

      res.on("end", () => {
        resolve(JSON.parse(data));
      });
    };

    let req: ClientRequest;

    if (options.address.startsWith("https")) {
      req = secureRequest(requestOptions, handler);
    } else {
      req = request(requestOptions, handler);
    }

    req.on("error", (error) => {
      reject(`Recieved error ${error}`);
    });

    req.write(body);
    req.end();
  });
}
