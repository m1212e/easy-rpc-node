import { createServer } from "http";
import { Server } from "./types/Server";

const servers: Server[] = [];

export function newServer(port: number) {
  const host = "localhost";

  const s = {
    port,
    callbacks: {},
  };
  servers.push(s);

  const requestListener = function (req, res) {
    res.writeHead(200);
    res.end("works");
  };

  const server = createServer(requestListener);
  server.listen(port, host, () => {
    console.log(`Server is running on port ${port}`);
  });

  return s.callbacks;
}
