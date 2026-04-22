# Ruby 业务后端接入

## 通用函数

```ruby
require 'net/http'
require 'json'
require 'uri'

def verify_captcha(token, endpoint, secret_key)
  uri = URI("#{endpoint}/api/v1/siteverify")
  http = Net::HTTP.new(uri.host, uri.port)
  http.use_ssl = (uri.scheme == 'https')
  http.open_timeout = 5
  http.read_timeout = 5

  req = Net::HTTP::Post.new(uri.path, 'Content-Type' => 'application/json')
  req.body = { token: token, secret_key: secret_key }.to_json
  resp = http.request(req)
  JSON.parse(resp.body)['success'] == true
rescue
  false
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
    ok = verify_captcha(params[:captcha_token],
                       ENV['CAPTCHA_ENDPOINT'],
                       ENV['CAPTCHA_SECRET_KEY'])
    render json: { error: '验证码校验失败' }, status: :forbidden unless ok
  end
end
```

## Sinatra

```ruby
require 'sinatra'
require 'sinatra/json'

post '/api/login' do
  body = JSON.parse(request.body.read)
  unless verify_captcha(body['captcha_token'], ENV['CAPTCHA_ENDPOINT'], ENV['CAPTCHA_SECRET_KEY'])
    halt 403, json(error: '验证码校验失败')
  end
  json(success: true)
end
```
