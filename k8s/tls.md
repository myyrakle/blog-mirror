# 대시보드 TLS

관리자 대시보드는 HTTP Basic 인증을 씁니다. 평문 HTTP로 노출하면
**자격증명이 매 요청마다 그대로 전송됩니다.**

이 자격증명으로 가능한 일:

- 복제 대상 카테고리 변경 (무엇이 공개 블로그에 발행될지 결정)
- 수집된 모든 글 본문과 작업 로그 열람
- 애플리케이션이 가진 GitHub 토큰으로 커밋·푸시 트리거

## 현재 상태

`public-gateway`에 HTTP 80 리스너 하나뿐입니다.

```bash
kubectl get gateway public-gateway -n default -o jsonpath='{.spec.listeners}'
# [{"name":"http","port":80,"protocol":"HTTP",...}]
```

cert-manager는 설치돼 있으나 **ClusterIssuer가 없어** 인증서를 발급할 수
없습니다. 아래 1번을 먼저 해야 합니다.

```bash
kubectl get clusterissuer
# No resources found
```

## 가장 간단한 대안 — 노출하지 않기

외부 접근이 꼭 필요하지 않다면 HTTPRoute를 지우고 포트포워딩으로 씁니다.
TLS도, 인증서 관리도 필요 없습니다.

```bash
kubectl delete -f k8s/httproute.yaml
kubectl port-forward -n default svc/blog-mirror 8080:80
# → http://localhost:8080
```

## HTTPS로 노출하기

### 1. ClusterIssuer 만들기

Let's Encrypt HTTP-01을 쓰려면 도메인이 외부에서 80포트로 닿아야 합니다.

```yaml
apiVersion: cert-manager.io/v1
kind: ClusterIssuer
metadata:
  name: letsencrypt
spec:
  acme:
    server: https://acme-v02.api.letsencrypt.org/directory
    email: you@example.com
    privateKeySecretRef:
      name: letsencrypt-account-key
    solvers:
      - http01:
          gatewayHTTPRoute:
            parentRefs:
              - name: public-gateway
                namespace: default
                kind: Gateway
```

### 2. 인증서 발급

```yaml
apiVersion: cert-manager.io/v1
kind: Certificate
metadata:
  name: blog-mirror-tls
  namespace: default
spec:
  secretName: blog-mirror-tls
  issuerRef:
    name: letsencrypt
    kind: ClusterIssuer
  dnsNames:
    - blog-mirror.myyrakle.com
```

### 3. Gateway에 HTTPS 리스너 추가

> `public-gateway`는 `nginx-test`, `whoami-test` 등 **다른 라우트와 공유**합니다.
> 통째로 `apply` 하지 말고 리스너만 덧붙이세요.

```bash
kubectl patch gateway public-gateway -n default --type=json -p='[{
  "op": "add", "path": "/spec/listeners/-", "value": {
    "name": "https-blog-mirror",
    "port": 443,
    "protocol": "HTTPS",
    "hostname": "blog-mirror.myyrakle.com",
    "allowedRoutes": {"namespaces": {"from": "Same"}},
    "tls": {"mode": "Terminate", "certificateRefs": [{"name": "blog-mirror-tls"}]}
  }
}]'
```

### 4. HTTPRoute를 HTTPS 리스너에 붙이기

`k8s/httproute.yaml`의 `parentRefs`에 `sectionName`을 지정합니다.

```yaml
  parentRefs:
    - group: gateway.networking.k8s.io
      kind: Gateway
      name: public-gateway
      sectionName: https-blog-mirror
```

### 5. 확인

```bash
curl -sI https://blog-mirror.myyrakle.com/healthz
kubectl get certificate blog-mirror-tls -n default
```

HTTP 리스너로도 계속 닿는다면, 평문 접근을 막기 위해 80 → 443 리다이렉트
HTTPRoute를 따로 두거나 기존 HTTPRoute에서 HTTP 리스너 연결을 끊으세요.
