# Java 业务后端接入

## 通用工具类（JDK 11+）

```java
import java.net.URI;
import java.net.http.*;
import java.time.Duration;

public class CaptchaVerifier {
    private static final HttpClient CLIENT = HttpClient.newBuilder()
        .connectTimeout(Duration.ofSeconds(5)).build();

    public static boolean verify(String token, String endpoint, String secretKey) throws Exception {
        String body = String.format(
            "{\"token\":\"%s\",\"secret_key\":\"%s\"}", token, secretKey);
        HttpRequest req = HttpRequest.newBuilder()
            .uri(URI.create(endpoint + "/api/v1/siteverify"))
            .header("Content-Type", "application/json")
            .timeout(Duration.ofSeconds(5))
            .POST(HttpRequest.BodyPublishers.ofString(body))
            .build();
        HttpResponse<String> resp = CLIENT.send(req, HttpResponse.BodyHandlers.ofString());
        return resp.body().contains("\"success\":true");
    }
}
```

## Spring Boot（Controller）

```java
@RestController
public class LoginController {

    @Value("${captcha.endpoint}")
    private String endpoint;
    @Value("${captcha.secret-key}")
    private String secretKey;

    @PostMapping("/api/login")
    public ResponseEntity<?> login(@RequestBody LoginRequest req) throws Exception {
        if (!CaptchaVerifier.verify(req.getCaptchaToken(), endpoint, secretKey)) {
            return ResponseEntity.status(403).body(Map.of("error", "验证码校验失败"));
        }
        return ResponseEntity.ok(Map.of("success", true));
    }
}
```

## Spring Boot（自定义注解 + AOP）

```java
@Retention(RetentionPolicy.RUNTIME)
@Target(ElementType.METHOD)
public @interface RequireCaptcha {}

@Aspect @Component
public class CaptchaAspect {
    @Value("${captcha.endpoint}") private String endpoint;
    @Value("${captcha.secret-key}") private String secretKey;

    @Before("@annotation(RequireCaptcha)")
    public void check(JoinPoint jp) throws Exception {
        HttpServletRequest request = ((ServletRequestAttributes)
            RequestContextHolder.currentRequestAttributes()).getRequest();
        String token = request.getParameter("captcha_token");
        if (!CaptchaVerifier.verify(token, endpoint, secretKey)) {
            throw new ResponseStatusException(HttpStatus.FORBIDDEN, "验证码校验失败");
        }
    }
}

// 用法：在 controller 方法上标 @RequireCaptcha
```
