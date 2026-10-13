# 배포

## ⚠️ `kubectl apply -f k8s/` 를 그대로 실행하지 마세요

`deployment.yaml`의 hostPath는 **플레이스홀더**입니다.

```yaml
path: /path/to/blog-mirror-clone
```

그대로 적용하면 **동작 중인 배포의 볼륨 경로가 존재하지 않는 경로로 덮여**
파드가 뜨지 못합니다. 실제 경로는 환경마다 다르므로 저장소에 넣지 않습니다.

## 적용 절차

실제 경로로 치환해서 적용합니다.

```bash
REPO_PATH=/home/myyrakle/Codes/Rust/blog-mirror-clone

sed "s|path: /path/to/blog-mirror-clone|path: ${REPO_PATH}|" k8s/deployment.yaml \
  | kubectl apply -f -
```

현재 적용된 경로는 이렇게 확인합니다.

```bash
kubectl get deploy blog-mirror -n default \
  -o jsonpath='{.spec.template.spec.volumes[0].hostPath.path}'
```

## 나머지 리소스

hostPath가 없으므로 그대로 적용해도 됩니다.

```bash
kubectl apply -f k8s/httproute.yaml
```

`secret.yaml`은 값이 전부 플레이스홀더입니다. 실제 값을 채워 넣거나,
이미 떠 있는 Secret을 직접 수정하세요.

```bash
kubectl patch secret blog-mirror-secret -n default --type merge \
  -p '{"stringData":{"WEB_PASSWORD":"..."}}'
```

> `WEB_USERNAME`과 `WEB_PASSWORD`는 **둘 다** 설정하거나 **둘 다** 비워야
> 합니다. 하나만 있으면 기동을 거부합니다.

## 업그레이드

이미지만 바꿀 때는 전체 적용 대신 이쪽이 안전합니다.

```bash
kubectl set image deploy/blog-mirror blog-mirror=myyrakle/blog-mirror:v0.2.2 -n default
kubectl rollout status deploy/blog-mirror -n default
```

`strategy: Recreate`라서 구 파드가 먼저 내려간 뒤 새 파드가 뜹니다.
hostPath 볼륨을 두 파드가 동시에 쓰지 않게 하기 위한 설정입니다.
