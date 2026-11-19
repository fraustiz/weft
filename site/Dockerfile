# Static landing page for weft.frohub.eu, served by nginx on port 8080.
# The relay keeps /v1, /healthz and /relay through more specific Traefik rules.
FROM nginx:stable-alpine
COPY nginx.conf /etc/nginx/conf.d/default.conf
COPY index.html style.css tokens.css base.css app.js /usr/share/nginx/html/
COPY assets /usr/share/nginx/html/assets
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD wget -q -O /dev/null http://127.0.0.1:8080/ || exit 1
