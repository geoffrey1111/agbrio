export type ConnectionMethod="CLOUDFLARE"|"TAILSCALE_FUNNEL"|"TAILSCALE_SERVE"|"CUSTOM_HTTPS";
export function deploymentInstructions(method:ConnectionMethod,origin:string,port:number){
 const setup=method==='TAILSCALE_FUNNEL'?`Use my Tailscale account; configure a background Funnel to http://127.0.0.1:${port} with tailscale funnel --bg http://127.0.0.1:${port}. Obtain its actual HTTPS .ts.net origin.`:method==='TAILSCALE_SERVE'?`Use my Tailscale account; configure tailscale serve --bg http://127.0.0.1:${port}. The phone must join the same tailnet. Obtain its actual HTTPS .ts.net origin.`:method==='CLOUDFLARE'?`Use my own Cloudflare domain/named Tunnel, forwarding HTTPS traffic to http://127.0.0.1:${port}. Preserve the public Host header. Do not replace other tunnel routes.`:`Use my existing HTTPS reverse proxy to http://127.0.0.1:${port}; preserve the public Host header.`;
 return `Configure Agbrio (Agent Bridge) on this Windows PC for my own phone.
Repository: https://github.com/geoffrey1111/agbrio
Method: ${method}
HTTPS origin: ${origin.trim()||'discover from my chosen service; do not invent an address'}
${setup}

Read the current release README and docs/SETUP.md first. Inspect the real installation and Host port. Install needed official dependencies and complete reversible setup. Use my accounts; ask me only for sign-in, service permissions or required administrator consent. Never ask me to paste secrets.
Keep existing Codex, Agbrio sessions, paired devices and other services. Do not install/start a second Codex Desktop or restart my working agent. Keep TLS, Host/Origin and device pairing authentication enabled. Cloudflare Access is optional; paired-device authorization is always required.
Submit the verified method/address using scripts/configure-mobile.ps1, or desktop Settings → Devices → Connection → Verify and save. This verifies the selected HTTPS entry against this exact Host before committing. Failed setup retains the old configuration. After READY, give me /mobile on that HTTPS origin; I generate a one-use pairing code in desktop Devices, enter it on the phone and add the PWA to the home screen.
Validate unauthenticated writes are refused and one disposable handoff works. Report actual checks and remaining account steps. Do not call browser emulation physical-iPhone acceptance.`;
}
