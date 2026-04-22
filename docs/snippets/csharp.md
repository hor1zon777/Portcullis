# C# / ASP.NET Core 业务后端接入

## 通用服务

```csharp
public class CaptchaService {
    private readonly HttpClient _http;
    private readonly string _endpoint;
    private readonly string _secretKey;

    public CaptchaService(HttpClient http, IConfiguration config) {
        _http = http;
        _endpoint = config["Captcha:Endpoint"]!;
        _secretKey = config["Captcha:SecretKey"]!;
    }

    public async Task<bool> VerifyAsync(string token, CancellationToken ct = default) {
        var payload = JsonContent.Create(new {
            token,
            secret_key = _secretKey
        });
        var resp = await _http.PostAsync($"{_endpoint}/api/v1/siteverify", payload, ct);
        var data = await resp.Content.ReadFromJsonAsync<SiteVerifyResponse>(cancellationToken: ct);
        return data?.Success == true;
    }

    private record SiteVerifyResponse(bool Success);
}
```

## 注册（Program.cs / Startup）

```csharp
builder.Services.AddHttpClient<CaptchaService>(c => {
    c.Timeout = TimeSpan.FromSeconds(5);
});
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
        if (!await _captcha.VerifyAsync(req.CaptchaToken)) {
            return StatusCode(403, new { error = "验证码校验失败" });
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
        if (!await svc.VerifyAsync(token)) {
            ctx.Result = new ObjectResult(new { error = "验证码校验失败" }) { StatusCode = 403 };
            return;
        }
        await next();
    }
}

// 用法：[RequireCaptcha] 标到 Controller 方法
```
