// Opt-in real Engine regression used by the Rust isolation test. All homes,
// packages, credentials and logs are fixtures under its temporary directory.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
const root = process.argv[2];
if (!root || !root.includes("/")) throw new Error("Missing fixture root");
const children: ReturnType<typeof Bun.spawn>[] = [];
const servers: ReturnType<typeof Bun.serve>[] = [];
const fixtures: Array<{ package: string; env: Record<string,string|undefined>; port: number; data: string }> = [];
const timeout = setTimeout(() => { for (const c of children) c.kill(); process.exit(1); }, 60000);
try {
  for (const id of ["smoke-a","smoke-b"]) {
    const profile = join(root,id), data = join(profile,".opencodex"), codex = join(profile,"codex-home");
    mkdirSync(data); mkdirSync(codex);
    const mock = Bun.serve({ port:0,hostname:"127.0.0.1",fetch: () => Response.json({data:[{id:"fixture-model"}]}) }); servers.push(mock);
    const reservation = Bun.serve({port:0,hostname:"127.0.0.1",fetch:()=>new Response("reserved")});
    const port = reservation.port!; reservation.stop(true);
    writeFileSync(join(data,"config.json"),JSON.stringify({port,hostname:"127.0.0.1",providers:{fixture:{baseUrl:`http://127.0.0.1:${mock.port}/v1`,allowPrivateNetwork:true,authMode:"local",adapter:"openai-responses",models:["fixture-model"]}},defaultProvider:"fixture",clientIntegrations:{codex:false,claudeCode:false,claudeDesktop:false,opencode:false,grok:false},codexAutoStart:false}));
    writeFileSync(join(codex,"config.toml"),'model_provider = "openai"\n');
    const token = `e30.${Buffer.from(JSON.stringify({exp:4102444800,"https://api.openai.com/auth":{chatgpt_account_id:"fixture-account"}})).toString("base64url")}.fixture`;
    writeFileSync(join(codex,"auth.json"),JSON.stringify({tokens:{access_token:token,id_token:token,refresh_token:"fixture-refresh"}}));
    const packageRoot = join(profile,"node_modules/@bitkyc08/opencodex");
    const env = {...process.env,HOME:profile,USERPROFILE:profile,OPENCODEX_HOME:data,CODEX_HOME:codex,CODEX_SQLITE_HOME:codex,OPENCODEX_PACKAGE_ROOT:packageRoot,NO_COLOR:"1"};
    delete env.HTTP_PROXY; delete env.HTTPS_PROXY; delete env.ALL_PROXY;
    const output = Bun.file(join(profile,"smoke.log"));
    const child = Bun.spawn([process.execPath,join(packageRoot,"src/cli/index.ts"),"start","--port",String(port)],{env,stdin:"ignore",stdout:output,stderr:output});
    children.push(child); fixtures.push({package:packageRoot,env,port,data});
    let healthy = false;
    for(let i=0;i<100;i++) {
      if(child.exitCode !== null) break;
      const health = await fetch(`http://127.0.0.1:${port}/healthz`,{signal:AbortSignal.timeout(500)}).then(r=>r.json()).catch(()=>null);
      if(health?.pid === child.pid) { healthy=true; break; }
      await Bun.sleep(200);
    }
    if(!healthy) throw new Error("Engine fixture failed to start: " + readFileSync(join(profile,"smoke.log"),"utf8"));
    const identity = JSON.parse(readFileSync(join(data,"runtime-port.json"),"utf8"));
    if(identity.port !== port || identity.pid !== child.pid) throw new Error("Runtime identity mismatch");
    const inspect = Bun.spawn([process.execPath,"--eval",`const m=await import(${JSON.stringify(join(packageRoot,"src/service.ts"))}); const p=m.buildPlist([]); if(!p.includes("com.opencodex.proxy.${id}") || !p.includes("<key>HOME</key>")) throw new Error("service not scoped"); console.log(JSON.stringify(m.diagnoseService()));`],{env,stdout:"pipe",stderr:"pipe"});
    const [code,out,err] = await Promise.all([inspect.exited,new Response(inspect.stdout).text(),new Response(inspect.stderr).text()]);
    if(code !== 0) throw new Error("Service definition: "+err);
    const status=JSON.parse(out); if(status.installed || status.conflict) throw new Error("Fixture saw another instance service");
  }
  const [first,second] = fixtures;
  async function integrate(fixture: typeof first, action: string) {
    const child = Bun.spawn([process.execPath,join(import.meta.dir,"manager-instance-integration.ts"),action,String(fixture.port)],
      {env:{...fixture.env,OPENCODEX_MANAGER_SOURCE_HOME:fixture.data,OPENCODEX_MANAGER_DEFAULT_INSTANCE:"1"},stdout:"pipe",stderr:"pipe"});
    const [code,out,err] = await Promise.all([child.exited,new Response(child.stdout).text(),new Response(child.stderr).text()]);
    if(code !== 0) throw new Error("Isolated integration failed: "+out+err);
  }
  await integrate(first,"sync");
  const firstConfig = readFileSync(join(first.env.CODEX_HOME!,"config.toml"),"utf8");
  await integrate(second,"sync");
  if(readFileSync(join(first.env.CODEX_HOME!,"config.toml"),"utf8") !== firstConfig) throw new Error("Sync B changed A");
  if(!readFileSync(join(second.env.CODEX_HOME!,"opencodex-catalog.json"),"utf8").includes("fixture-model")) throw new Error("Missing isolated catalog");
  await integrate(second,"restore");
  if(readFileSync(join(first.env.CODEX_HOME!,"config.toml"),"utf8") !== firstConfig) throw new Error("Restore B changed A");
  const stop = Bun.spawn([process.execPath,join(first.package,"src/cli/index.ts"),"stop"],{env:first.env,stdout:"pipe",stderr:"pipe"});
  const [code,err]=await Promise.all([stop.exited,new Response(stop.stderr).text(),new Response(stop.stdout).text()]);
  if(code !== 0) throw new Error("First instance stop failed: "+err);
  const health = await fetch(`http://127.0.0.1:${second.port}/healthz`).then(r=>r.json());
  if(health.pid !== children[1].pid) throw new Error("Stopping first instance affected second");
  if(readFileSync(join(first.data,"admin-api-token"),"utf8") === readFileSync(join(second.data,"admin-api-token"),"utf8")) throw new Error("Admin credentials were shared");
  console.log("Two real Engines: independent ports, PIDs, admin tokens and service definitions; stopping A leaves B healthy.");
} finally {
  clearTimeout(timeout);
  for(const child of children) if(child.exitCode === null) child.kill();
  await Promise.all(children.map(c=>c.exited));
  for(const server of servers) server.stop(true);
}
