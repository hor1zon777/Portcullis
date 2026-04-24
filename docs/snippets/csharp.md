# C# / ASP.NET Core 业务后端接入

> v1.4+ 新增 `client_ip` / `user_agent` 字段用于 opt-in 身份绑定。下面的示例都已携带；**未启用绑定的站点会忽略这两个字段**，保持向后兼容。

## 通用服务

```csharp
using System.Net.Http.Json;

public class CaptchaService {
    private readonly HttpClient _http;
    private readonly string _endpoint;
    private readonly string _secretKey;

    public CaptchaService(HttpClient http, IConfiguration config) {
        _http = http;
        _endpoint = config["Captcha:Endpoint"]!;
        _secretKey = config["Captcha:SecretKey"]!;
    }

    public async Task<(bool Success, string? Error)> VerifyAsync(
        string token,
        string? clientIp = null,      // v1.4+
        string? userAgent = null,     // v1.4+
        CancellationToken ct = default
    ) {
        var payload = new Dictionary<string, string> {
            ["token"] = token,
            ["secret_key"] = _secretKey,
        };
        if (clientIp is not null) payload["client_ip"] = clientIp;
        if (userAgent is not null) payload["user_agent"] = userAgent;

        var resp = await _http.PostAsJsonAsync($"{_endpoint}/api/v1/siteverify", payload, ct);
        var data = await resp.Content.ReadFromJsonAsync<SiteVerifyResponse>(cancellationToken: ct);
        return (data?.Success == true, data?.Error);
    }

    private record SiteVerifyResponse(bool Success, string? Error);
}
```

## 注册（Program.cs / Startup）

```csharp
builder.Services.AddHttpClient<CaptchaService>(c => {
    c.Timeout = TimeSpan.FromSeconds(5);
});

// 启用 bind_token_to_ip 时：配置 ForwardedHeaders 中间件让 HttpContext.Connection.RemoteIpAddress 反映真实客户端
builder.Services.Configure<ForwardedHeadersOptions>(opts => {
    opts.ForwardedHeaders = ForwardedHeaders.XForwardedFor | ForwardedHeaders.XForwardedProto;
    opts.KnownProxies.Clear();
    opts.KnownNetworks.Clear();
});
// Program.cs 顶部加上：app.UseForwardedHeaders();
```

## Controller 用法

```csharp
[ApiController]
[Route("api")]
public class AuthController : ControllerBase {
    private readonly CaptchaService _captcha;
    public AuthController(CaptchaService captcha) => _captcha = captcha;

    [HttpPost("login")]
    public async Task<IActionResult> Login([FromBody] LoginRequest req) {
        var clientIp = HttpContext.Connection.RemoteIpAddress?.ToString();
        var userAgent = Request.Headers.UserAgent.ToString();

        var (ok, error) = await _captcha.VerifyAsync(req.CaptchaToken, clientIp, userAgent);
        if (!ok) {
            return StatusCode(403, new { error = error ?? "验证码校验失败" });
        }
        return Ok(new { success = true });
    }
}

public record LoginRequest(string Username, string Password, string CaptchaToken);
```

## Action Filter（可复用）

```csharp
public class RequireCaptchaAttribute : ActionFilterAttribute {
    public override async Task OnActionExecutionAsync(ActionExecutingContext ctx, ActionExecutionDelegate next) {
        var svc = ctx.HttpContext.RequestServices.GetRequiredService<CaptchaService>();
        var token = ctx.HttpContext.Request.Form["captcha_token"].ToString();
        var clientIp = ctx.HttpContext.Connection.RemoteIpAddress?.ToString();
        var userAgent = ctx.HttpContext.Request.Headers.UserAgent.ToString();

        var (ok, error) = await svc.VerifyAsync(token, clientIp, userAgent);
        if (!ok) {
            ctx.Result = new ObjectResult(new { error = error ?? "验证码校验失败" }) { StatusCode = 403 };
            return;
        }
        await next();
    }
}

// 用法：[RequireCaptcha] 标到 Controller 方法
```

## 生产注意事项

- 启用 `bind_token_to_ip` 时必须配置 `ForwardedHeaders` 中间件 + `UseForwardedHeaders()`，否则 `HttpContext.Connection.RemoteIpAddress` 是反代 IP
- v1.5+ 的 `secret_key` 是一次性明文；建议 `IConfiguration` 从 `appsettings.Production.json` + Azure KeyVault / AWS Secrets Manager 加载
- 启用身份绑定时请确认反代正确透传（详见 [`docs/DEPLOY.md`](../DEPLOY.md) §7.1）

