# Ruby 业务后端接入

> v1.4+ 新增 `client_ip` / `user_agent` 字段用于 opt-in 身份绑定。下面的示例都已携带；**未启用绑定的站点会忽略这两个字段**，保持向后兼容。

## 通用函数

```ruby
require 'net/http'
require 'json'
require 'uri'

def verify_captcha(token, endpoint, secret_key, client_ip: nil, user_agent: nil)
  uri = URI("#{endpoint}/api/v1/siteverify")
  http = Net::HTTP.new(uri.host, uri.port)
  http.use_ssl = (uri.scheme == 'https')
  http.open_timeout = 5
  http.read_timeout = 5

  payload = { token: token, secret_key: secret_key }
  payload[:client_ip] = client_ip unless client_ip.nil?
  payload[:user_agent] = user_agent unless user_agent.nil?

  req = Net::HTTP::Post.new(uri.path, 'Content-Type' => 'application/json')
  req.body = payload.to_json
  resp = http.request(req)
  data = JSON.parse(resp.body)
  { success: data['success'] == true, error: data['error'] }
rescue => e
  { success: false, error: e.message }
end
```

## Rails Controller

```ruby
class SessionsController < ApplicationController
  before_action :verify_captcha!, only: [:create]

  def create
    # ... 业务逻辑
    render json: { success: true }
  end

  private

  def verify_captcha!
    # Rails 4+ request.remote_ip 默认会解析 X-Forwarded-For
    # 需配合 config.action_dispatch.trusted_proxies 设置可信代理网段
    result = verify_captcha(
      params[:captcha_token],
      ENV['CAPTCHA_ENDPOINT'],
      ENV['CAPTCHA_SECRET_KEY'],
      client_ip: request.remote_ip,
      user_agent: request.user_agent
    )
    unless result[:success]
      render json: { error: result[:error] || '验证码校验失败' }, status: :forbidden
    end
  end
end
```

## Sinatra

```ruby
require 'sinatra'
require 'sinatra/json'

post '/api/login' do
  body = JSON.parse(request.body.read)
  # Sinatra 不会默认解析 XFF，这里手动取
  client_ip = (request.env['HTTP_X_FORWARDED_FOR'] || '').split(',').first&.strip ||
              request.env['REMOTE_ADDR']
  result = verify_captcha(
    body['captcha_token'],
    ENV['CAPTCHA_ENDPOINT'],
    ENV['CAPTCHA_SECRET_KEY'],
    client_ip: client_ip,
    user_agent: request.env['HTTP_USER_AGENT']
  )
  unless result[:success]
    halt 403, json(error: result[:error] || '验证码校验失败')
  end
  json(success: true)
end
```

## 生产注意事项

- Rails：在 `config/application.rb` 里设置 `config.action_dispatch.trusted_proxies = [IPAddr.new('10.0.0.0/8'), ...]`，让 `request.remote_ip` 返回真实客户端 IP
- 启用 `bind_token_to_ip` 时确认反代正确透传（详见 [`docs/DEPLOY.md`](../DEPLOY.md) §7.1）
- v1.5+ 的 `secret_key` 仅在创建站点时一次性返回；建议存入 `credentials.yml.enc` 或 Rails.application.credentials

