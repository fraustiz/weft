# Static landing page for weft.frohub.eu (French at /, English at /en/), served
# by nginx on port 8080. The relay keeps /v1, /healthz and /relay through more
# specific Traefik rules.
FROM nginx:stable-alpine
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY security-headers.conf /etc/nginx/snippets/security-headers.conf
COPY index.html style.css tokens.css base.css app.js /usr/share/nginx/html/
COPY en /usr/share/nginx/html/en
COPY assets /usr/share/nginx/html/assets
# Version CSS and JS URLs by content so a CDN never serves a stale file.
RUN cd /usr/share/nginx/html \
 && css=$(cat style.css tokens.css base.css | sha1sum | cut -c1-10) \
 && js=$(sha1sum app.js | cut -c1-10) \
 && sed -i "s/tokens\.css\")/tokens.css?v=$css\")/; s/base\.css\")/base.css?v=$css\")/" style.css \
 && sed -i "s/style\.css\"/style.css?v=$css\"/; s/app\.js\"/app.js?v=$js\"/" index.html en/index.html
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD wget -q -O /dev/null http://127.0.0.1:8080/ || exit 1
