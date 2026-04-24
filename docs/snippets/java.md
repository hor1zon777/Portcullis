# Java 业务后端接入

> v1.4+ 新增 `client_ip` / `user_agent` 字段用于 opt-in 身份绑定。下面的示例都已携带；**未启用绑定的站点会忽略这两个字段**，保持向后兼容。

## 通用工具类（JDK 11+）

```java
import java.net.URI;
import java.net.http.*;
import java.time.Duration;
import com.fasterxml.jackson.databind.ObjectMapper;
import java.util.*;

public class CaptchaVerifier {
    private static final HttpClient CLIENT = HttpClient.newBuilder()
        .connectTimeout(Duration.ofSeconds(5)).build();
    private static final ObjectMapper MAPPER = new ObjectMapper();

    public static Result verify(
        String token,
        String endpoint,
        String secretKey,
        String clientIp,      // v1.4+，可为 null
        String userAgent      // v1.4+，可为 null
    ) throws Exception {
        Map<String, String> payload = new HashMap<>();
        payload.put("token", token);
        payload.put("secret_key", secretKey);
        if (clientIp != null) payload.put("client_ip", clientIp);
        if (userAgent != null) payload.put("user_agent", userAgent);

        String body = MAPPER.writeValueAsString(payload);
        HttpRequest req = HttpRequest.newBuilder()
            .uri(URI.create(endpoint + "/api/v1/siteverify"))
            .header("Content-Type", "application/json")
            .timeout(Duration.ofSeconds(5))
            .POST(HttpRequest.BodyPublishers.ofString(body))
            .build();
        HttpResponse<String> resp = CLIENT.send(req, HttpResponse.BodyHandlers.ofString());
        Map<?,?> data = MAPPER.readValue(resp.body(), Map.class);
        boolean ok = Boolean.TRUE.equals(data.get("success"));
        return new Result(ok, (String) data.get("error"));
    }

    public record Result(boolean success, String error) {}
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
    public ResponseEntity<?> login(
        @RequestBody LoginRequest req,
        HttpServletRequest http  // 注入以获取 IP / UA
    ) throws Exception {
        String clientIp = resolveClientIp(http);
        String userAgent = http.getHeader("User-Agent");

        CaptchaVerifier.Result r = CaptchaVerifier.verify(
            req.getCaptchaToken(), endpoint, secretKey, clientIp, userAgent);
        if (!r.success()) {
            return ResponseEntity.status(403).body(
                Map.of("error", Optional.ofNullable(r.error()).orElse("验证码校验失败")));
        }
        return ResponseEntity.ok(Map.of("success", true));
    }

    /** 从反向代理 header 提取真实客户端 IP */
    private static String resolveClientIp(HttpServletRequest req) {
        String xff = req.getHeader("X-Forwarded-For");
        if (xff != null && !xff.isEmpty()) {
            return xff.split(",")[0].trim();
        }
        String xri = req.getHeader("X-Real-IP");
        if (xri != null && !xri.isEmpty()) return xri.trim();
        return req.getRemoteAddr();
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
        HttpServletRequest req = ((ServletRequestAttributes)
            RequestContextHolder.currentRequestAttributes()).getRequest();
        String token = req.getParameter("captcha_token");
        String xff = req.getHeader("X-Forwarded-For");
        String ip = (xff != null && !xff.isEmpty()) ? xff.split(",")[0].trim() : req.getRemoteAddr();
        String ua = req.getHeader("User-Agent");

        CaptchaVerifier.Result r = CaptchaVerifier.verify(token, endpoint, secretKey, ip, ua);
        if (!r.success()) {
            throw new ResponseStatusException(HttpStatus.FORBIDDEN,
                r.error() != null ? r.error() : "验证码校验失败");
        }
    }
}

// 用法：在 controller 方法上标 @RequireCaptcha
```

## 生产注意事项

- Spring Boot 若在反代后，需要 `server.forward-headers-strategy=native` 或显式解析 `X-Forwarded-For` 以获得真实 IP
- v1.5+ 的 `secret_key` 是一次性明文，创建站点时务必存入 Vault / 配置中心
- 启用 `bind_token_to_ip` 时确认反代正确透传 IP（详见 [`docs/DEPLOY.md`](../DEPLOY.md) §7.1）

