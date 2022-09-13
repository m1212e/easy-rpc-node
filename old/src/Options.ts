/**
 * Options to initialize an easy-rpc target
 */
export interface TargetOptions {
  /**
   * Which address should the request be sent to
   * (without port)
   */
  address: string;
  /**
   * At which port does the server listen for requests
   */
  port: number;
}

export interface ServerOptions {
  /**
   * At which port should the server listen for requests
   */
  port: number;
  /**
   * Which origins should be allowed inside the servers cors header.
   * e.g. http://localhost:1234
   */
  allowedCorsOrigins: string[];
}
