# PHP 业务后端接入

> v1.4+ 新增 `client_ip` / `user_agent` 字段用于 opt-in 身份绑定。下面的示例都已携带；**未启用绑定的站点会忽略这两个字段**，保持向后兼容。

## 通用函数（PHP 8+）

```php
<?php

function verifyCaptcha(
    string $token,
    string $endpoint,
    string $secretKey,
    ?string $clientIp = null,      // v1.4+
    ?string $userAgent = null      // v1.4+
): array {
    $payload = ['token' => $token, 'secret_key' => $secretKey];
    if ($clientIp !== null) {
        $payload['client_ip'] = $clientIp;
    }
    if ($userAgent !== null) {
        $payload['user_agent'] = $userAgent;
    }

    $ch = curl_init($endpoint . '/api/v1/siteverify');
    curl_setopt_array($ch, [
        CURLOPT_RETURNTRANSFER => true,
        CURLOPT_POST           => true,
        CURLOPT_TIMEOUT        => 5,
        CURLOPT_HTTPHEADER     => ['Content-Type: application/json'],
        CURLOPT_POSTFIELDS     => json_encode($payload),
    ]);
    $resp = curl_exec($ch);
    curl_close($ch);
    $data = json_decode($resp, true) ?: [];
    return [
        'success' => ($data['success'] ?? false) === true,
        'error'   => $data['error'] ?? null,
    ];
}

/** 从 X-Forwarded-For / X-Real-IP 取真实客户端 IP */
function clientIp(): string {
    if (!empty($_SERVER['HTTP_X_FORWARDED_FOR'])) {
        return trim(explode(',', $_SERVER['HTTP_X_FORWARDED_FOR'])[0]);
    }
    if (!empty($_SERVER['HTTP_X_REAL_IP'])) {
        return $_SERVER['HTTP_X_REAL_IP'];
    }
    return $_SERVER['REMOTE_ADDR'] ?? '';
}
```

## 原生 PHP

```php
<?php
header('Content-Type: application/json');

$body = json_decode(file_get_contents('php://input'), true);
$token = $body['captcha_token'] ?? '';

$res = verifyCaptcha(
    $token,
    getenv('CAPTCHA_ENDPOINT'),
    getenv('CAPTCHA_SECRET_KEY'),
    clientIp(),
    $_SERVER['HTTP_USER_AGENT'] ?? ''
);
if (!$res['success']) {
    http_response_code(403);
    echo json_encode(['error' => $res['error'] ?? '验证码校验失败']);
    exit;
}

echo json_encode(['success' => true]);
```

## Laravel 中间件

```php
<?php

namespace App\Http\Middleware;

use Closure;
use Illuminate\Http\Request;

class VerifyCaptcha {
    public function handle(Request $request, Closure $next) {
        $token = $request->input('captcha_token');
        $res = verifyCaptcha(
            $token,
            env('CAPTCHA_ENDPOINT'),
            env('CAPTCHA_SECRET_KEY'),
            $request->ip(),                         // Laravel 会自动解析 X-Forwarded-For，需配置 trustedproxies
            $request->header('User-Agent')
        );
        if (!$res['success']) {
            return response()->json(['error' => $res['error'] ?? '验证码校验失败'], 403);
        }
        return $next($request);
    }
}

// 注册到路由：Route::post('/login', ...)->middleware(VerifyCaptcha::class);
// 反代配置：app/Http/Middleware/TrustProxies.php 设置 protected $proxies = '*';
```

## Slim 4

```php
use Psr\Http\Message\{ServerRequestInterface, ResponseInterface};

$app->post('/login', function (ServerRequestInterface $req, ResponseInterface $res) {
    $body = $req->getParsedBody();
    $xff = $req->getHeaderLine('X-Forwarded-For');
    $clientIp = $xff ? trim(explode(',', $xff)[0]) : ($req->getServerParams()['REMOTE_ADDR'] ?? '');

    $result = verifyCaptcha(
        $body['captcha_token'] ?? '',
        getenv('CAPTCHA_ENDPOINT'),
        getenv('CAPTCHA_SECRET_KEY'),
        $clientIp,
        $req->getHeaderLine('User-Agent')
    );
    if (!$result['success']) {
        return $res->withStatus(403)->withJson(['error' => $result['error'] ?? '验证码校验失败']);
    }
    return $res->withJson(['success' => true]);
});
```

## 生产注意事项

- Laravel `$request->ip()` 需要在 `TrustProxies` 中间件信任反代；否则取到的是代理 IP
- 启用 `bind_token_to_ip` 时务必确认反代正确透传 IP（详见 [`docs/DEPLOY.md`](../DEPLOY.md) §7.1）
- v1.5+ 的 `secret_key` 只能在创建站点时一次性获取；存入 `.env` 或密钥管理器

