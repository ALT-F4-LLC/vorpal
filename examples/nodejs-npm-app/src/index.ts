import { Hono } from "hono";
import { serve } from "@hono/node-server";

export const add = (x: number, y: number) => x + y;

const app = new Hono();

app.get('/add/:x/:y', ctx => {
  const { x, y } = ctx.req.param();

  return ctx.text(`${add(parseInt(x), parseInt(y))}`);
});

serve({ fetch: app.fetch }, addr => {
  console.info(`Listening on '${addr.address}'.`);
});
