# PHP 业务后端接入

## 通用函数（PHP 8+）

```php
<?php

function verifyCaptcha(string $token, string $endpoint, string $secretKey): bool {
    $ch = curl_init($endpoint . '/api/v1/siteverify');
    curl_setopt_array($ch, [
        CURLOPT_RETURNTRANSFER => true,
        CURLOPT_POST           => true,
        CURLOPT_TIMEOUT        => 5,
        CURLOPT_HTTPHEADER     => ['Content-Type: application/json'],
        CURLOPT_POSTFIELDS     => json_encode([
            'token'      => $token,
            'secret_key' => $secretKey,
        ]),
    ]);
    $resp = curl_exec($ch);
    curl_close($ch);
    $data = json_decode($resp, true);
    return ($data['success'] ?? false) === true;
}
```

## 原生 PHP

```php
<?php
header('Content-Type: application/json');

$body = json_decode(file_get_contents('php://input'), true);
$token = $body['captcha_token'] ?? '';

if (!verifyCaptcha($token, getenv('CAPTCHA_ENDPOINT'), getenv('CAPTCHA_SECRET_KEY'))) {
    http_response_code(403);
    echo json_encode(['error' => '验证码校验失败']);
    exit;
}

echo json_encode(['success' => true]);
```

## Laravel 中间件

```php
<?php

namespace App\Http\Middleware;

use Closure;

class VerifyCaptcha {
    public function handle($request, Closure $next) {
        $token = $request->input('captcha_token');
        if (!verifyCaptcha($token, env('CAPTCHA_ENDPOINT'), env('CAPTCHA_SECRET_KEY'))) {
            return response()->json(['error' => '验证码校验失败'], 403);
        }
        return $next($request);
    }
}

// 注册到路由：Route::post('/login', ...)->middleware(VerifyCaptcha::class);
```

## Slim 4

```php
use Psr\Http\Message\{ServerRequestInterface, ResponseInterface};

$app->post('/login', function (ServerRequestInterface $req, ResponseInterface $res) {
    $body = $req->getParsedBody();
    if (!verifyCaptcha($body['captcha_token'] ?? '', getenv('CAPTCHA_ENDPOINT'), getenv('CAPTCHA_SECRET_KEY'))) {
        return $res->withStatus(403)->withJson(['error' => '验证码校验失败']);
    }
    return $res->withJson(['success' => true]);
});
```
