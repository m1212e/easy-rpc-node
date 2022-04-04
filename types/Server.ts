export interface Server {
  port: number;
  callbacks: {
    [key: string]: (any) => any;
  };
}
