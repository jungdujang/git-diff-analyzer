use clap::Parser;
use dotenv::dotenv;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;
use anyhow::{Result, anyhow};
use tokio;

#[derive(Parser)]
#[command(name = "git-diff-analyzer")]
#[command(about = "Git diff를 분석하여 변경점을 요약하는 도구")]
struct Args {
    /// 프로젝트 이름
    #[arg(short, long)]
    project: String,
    
    /// 이전 태그 (커밋 분석 시 선택사항)
    #[arg(short, long)]
    from_tag: Option<String>,
    
    /// 이후 태그 (커밋 분석 시 선택사항)
    #[arg(short, long)]
    to_tag: Option<String>,
    
    /// 분석할 커밋 해시 (단일 커밋 분석 시 사용)
    #[arg(short, long)]
    commit: Option<String>,
    
    /// 프로젝트 경로 (선택사항, 기본값: ./repositories/{project})
    #[arg(long)]
    path: Option<String>,
}

#[derive(Serialize)]
struct OpenAIRequest {
    model: String,
    messages: Vec<Message>,
    max_tokens: u32,
    temperature: f32,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct OpenAIResponse {
    choices: Vec<Choice>,
}

#[derive(Deserialize)]
struct Choice {
    message: MessageResponse,
}

#[derive(Deserialize)]
struct MessageResponse {
    content: String,
}

async fn get_git_diff(project_path: &str, from_tag: &str, to_tag: &str) -> Result<String> {
    println!("{}에서 {} -> {} git diff 생성 중...", project_path, from_tag, to_tag);
    
    let output = Command::new("git")
        .current_dir(project_path)
        .args(&[
            "diff", 
            from_tag, 
            to_tag,
            "--",
            ":!package-lock.json",      // npm lock file 제외
            ":!yarn.lock",              // yarn lock file 제외
            ":!pnpm-lock.yaml",         // pnpm lock file 제외
            ":!composer.lock",          // composer lock file 제외
            ":!Gemfile.lock",           // ruby lock file 제외
            ":!poetry.lock",            // python poetry lock file 제외
            ":!Pipfile.lock",           // python pipenv lock file 제외
            ":!go.sum",                 // go modules checksum 제외
            ":!*.min.js",               // 압축된 JS 파일 제외
            ":!*.min.css",              // 압축된 CSS 파일 제외
            ":!dist/*",                 // 빌드 결과물 제외
            ":!build/*",                // 빌드 결과물 제외
        ])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Git diff 실행 실패: {}", stderr));
    }
    
    let diff_content = String::from_utf8_lossy(&output.stdout).to_string();
    
    // 추가적으로 대용량 자동 생성 파일들을 필터링
    let filtered_diff = filter_large_generated_files(&diff_content);
    
    println!("Lock 파일 및 자동 생성 파일들이 제외된 diff가 생성되었습니다.");
    
    Ok(filtered_diff)
}

async fn get_commit_diff(project_path: &str, commit_hash: &str) -> Result<String> {
    println!("{}에서 커밋 {} 변경사항 분석 중...", project_path, commit_hash);
    
    let output = Command::new("git")
        .current_dir(project_path)
        .args(&[
            "show", 
            "--format=fuller",
            commit_hash,
            "--",
            ":!package-lock.json",      // npm lock file 제외
            ":!yarn.lock",              // yarn lock file 제외
            ":!pnpm-lock.yaml",         // pnpm lock file 제외
            ":!composer.lock",          // composer lock file 제외
            ":!Gemfile.lock",           // ruby lock file 제외
            ":!poetry.lock",            // python poetry lock file 제외
            ":!Pipfile.lock",           // python pipenv lock file 제외
            ":!go.sum",                 // go modules checksum 제외
            ":!*.min.js",               // 압축된 JS 파일 제외
            ":!*.min.css",              // 압축된 CSS 파일 제외
            ":!dist/*",                 // 빌드 결과물 제외
            ":!build/*",                // 빌드 결과물 제외
        ])
        .output()?;
    
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("Git show 실행 실패: {}", stderr));
    }
    
    let diff_content = String::from_utf8_lossy(&output.stdout).to_string();
    
    // 추가적으로 대용량 자동 생성 파일들을 필터링
    let filtered_diff = filter_large_generated_files(&diff_content);
    
    println!("Lock 파일 및 자동 생성 파일들이 제외된 커밋 diff가 생성되었습니다.");
    
    Ok(filtered_diff)
}

fn filter_large_generated_files(diff_content: &str) -> String {
    let lines: Vec<&str> = diff_content.lines().collect();
    let mut filtered_lines = Vec::new();
    let mut skip_file = false;
    let mut current_file = String::new();
    
    for line in lines {
        if line.starts_with("diff --git") {
            // 새 파일 시작
            skip_file = false;
            if let Some(file_path) = line.split_whitespace().nth(3) {
                current_file = file_path.trim_start_matches("b/").to_string();
                
                // 제외할 파일 패턴들
                if should_skip_file(&current_file) {
                    skip_file = true;
                    continue;
                }
            }
        }
        
        if !skip_file {
            filtered_lines.push(line);
        }
    }
    
    filtered_lines.join("\n")
}

fn should_skip_file(file_path: &str) -> bool {
    let skip_patterns = [
        // Lock files
        "package-lock.json",
        "yarn.lock", 
        "pnpm-lock.yaml",
        "composer.lock",
        "Gemfile.lock",
        "poetry.lock",
        "Pipfile.lock",
        "go.sum",
        
        // Generated/compiled files
        ".min.js",
        ".min.css",
        ".bundle.js",
        ".bundle.css",
        
        // Build directories
        "dist/",
        "build/",
        "output/",
        "out/",
        
        // Documentation auto-generated
        "CHANGELOG.md",
        
        // IDE/Editor files
        ".vscode/",
        ".idea/",
        
        // OS files
        ".DS_Store",
        "Thumbs.db",
        
        // Large data files
        ".json.map",
        ".js.map",
        ".css.map",
    ];
    
    skip_patterns.iter().any(|pattern| {
        file_path.contains(pattern) || file_path.ends_with(pattern)
    })
}

fn estimate_tokens(text: &str) -> usize {
    // 대략적인 토큰 추정 (영어: 4글자≈1토큰, 한국어: 1글자≈1토큰)
    let korean_chars = text.chars().filter(|c| *c >= '가' && *c <= '힣').count();
    let other_chars = text.chars().count() - korean_chars;
    
    korean_chars + (other_chars / 4)
}

fn smart_summarize_diff(diff_content: &str, max_tokens: usize) -> String {
    let mut summary = String::new();
    let lines: Vec<&str> = diff_content.lines().collect();
    
    // 기본 통계
    let _total_lines = lines.len();
    let added_lines = lines.iter().filter(|line| line.starts_with('+')).count();
    let removed_lines = lines.iter().filter(|line| line.starts_with('-')).count();
    let modified_files = lines.iter().filter(|line| line.starts_with("diff ")).count();
    
    summary.push_str(&format!("=== 통계 ===\n파일 {}개, +{} -{} 라인\n\n", modified_files, added_lines, removed_lines));
    
    // 전체 diff 내용을 토큰 제한에 맞춰 포함
    let stats_tokens = estimate_tokens(&summary);
    let available_tokens = max_tokens - stats_tokens;
    
    // diff 내용을 토큰 제한에 맞춰 자르기
    let mut remaining_content = diff_content;
    let mut truncated_content = String::new();
    
    for line in remaining_content.lines() {
        let line_with_newline = format!("{}\n", line);
        if estimate_tokens(&(truncated_content.clone() + &line_with_newline)) < available_tokens {
            truncated_content.push_str(&line_with_newline);
        } else {
            truncated_content.push_str("... (토큰 제한으로 나머지 내용 생략)\n");
            break;
        }
    }
    
    summary.push_str(&truncated_content);
    summary
}

async fn analyze_diff_with_openai(diff_content: &str, api_key: &str, project: &str, from_tag: &str, to_tag: &str) -> Result<String> {
    println!("OpenAI API로 diff 분석 중...");
    
    let client = Client::new();
    
    // 프롬프트 토큰 추정 (약 800 토큰)
    let prompt_base_tokens = 800;
    let max_content_tokens = 120000 - prompt_base_tokens - 4000; // GPT-4 Turbo: 128k, 응답용 4k 예약
    
    // diff 내용 처리
    let analysis_content = if estimate_tokens(diff_content) > max_content_tokens {
        println!("Diff 내용이 큽니다. 스마트 요약해서 분석합니다...");
        smart_summarize_diff(diff_content, max_content_tokens)
    } else {
        diff_content.to_string()
    };
    
    println!("예상 토큰 사용량: {} / 128,000", estimate_tokens(&analysis_content) + prompt_base_tokens);
    
    let prompt = format!(
        "{}의 {} → {} 변경사항을 라이브러리 사용자 관점에서 분석해주세요.

**분석 목적**: 라이브러리를 빌드 후 사용하는 개발자가 버전 업데이트 시 발생할 수 있는 사이드 이펙트를 사전에 파악하여 방지

**분석 기준**:
- Chromium M38+ 버전 기준 (구체적인 API별 호환성 체크 필수)
- 라이브러리 빌드 후 사용자에게 실제 영향을 주는 변경사항만 분석
- 코드 스타일, 주석 등 동작에 영향 없는 변경사항은 제외
- API 변경, 동작 로직 변경, 성능 영향, 최적화 등 실질적 변경사항 중심
- 사용자 영향이 없더라도 동작 변경이 있으면 반드시 분석
- 각 변경사항마다 파일명과 실제 코드 변경 내용을 포함

**🚨 중요 API 호환성 체크리스트** (반드시 확인):
- **HTMLMediaElement.play()**: Chrome 50+에서 Promise 반환, 이전 버전(M38-M49)에서는 void 반환 → .catch() 사용 시 에러!
- **fetch()**: Chrome 42+ (M38에서는 사용 불가)
- **Promise**: Chrome 32+ (M38에서 지원)
- **async/await**: Chrome 55+ (M38에서는 사용 불가)
- **ResizeObserver**: Chrome 64+ (M38에서는 사용 불가)
- **IntersectionObserver**: Chrome 51+ (M38에서는 사용 불가)
- **Object.assign()**: Chrome 45+ (M38에서는 사용 불가)
- **Array.includes()**: Chrome 47+ (M38에서는 사용 불가)
- **Array.find()/findIndex()**: Chrome 45+ (M38에서는 사용 불가)
- **String.includes/startsWith/endsWith**: Chrome 41+ (M38에서는 사용 불가)
- **Map/Set**: Chrome 38+ (M38에서 지원)
- **for...of**: Chrome 38+ (M38에서 지원)

**중요**: 코드에서 이런 API들이 사용되면 반드시 브라우저 호환성을 체크하고, 문제가 있으면 **높은 리스크**로 분류하세요!

다음 형식으로 마크다운 분석 보고서를 작성해주세요:

# {} 변경사항 분석 ({} → {}) - 사이드 이펙트 분석

## 📊 개요
- 분석 대상: {} {} → {}
- 분석 목적: 라이브러리 사용자의 사이드 이펙트 방지
- 분석 기준: Chromium M38+ 버전 기준, 동작 변경 중심

## 🌐 크로스브라우징 영향 분석 (Chromium M38+ 기준)

실제 동작 변경이 있는 파일들을 분석하여 각 변경사항별로:
- 변경된 파일명과 구체적인 코드 변경 내용
- **구체적인 브라우저 호환성 문제** (상기 체크리스트 기준으로 정확히 분석)
- 호환성 문제가 있다면 **어떤 브라우저 버전에서 에러가 발생하는지** 명시
- 안전한 코딩 패턴 제시

### 🚨 호환성 경고 (발견 시)
각 문제별로:
**문제 코드**: `구체적인 코드`
**문제점**: Chrome M38-M49에서 HTMLMediaElement.play()는 void를 반환하므로 .catch() 호출 시 TypeError 발생
**안전한 코드**:
```javascript
const playPromise = media.play();
if (playPromise !== undefined) {{
  playPromise.catch(/* 에러 처리 */);
}}
```

## 🎬 미디어 재생 영향 분석

미디어 재생 관련 변경사항이 있다면:
- 변경된 파일명과 재생 로직 변경 내용
- 관련 미디어 기술 배경 설명 (코덱, 스트리밍, DRM 등)
- MediaSource, HTMLMediaElement 등 미디어 API 사용 여부
- 재생 품질, 성능, 안정성에 미치는 실제 영향
- 미디어 API 호환성 문제 (위 체크리스트 기준)

## 🔧 라이브러리 사용자 영향 분석

API 변경, 동작 변경, 성능 최적화 등이 있다면:
- 변경된 파일명과 구체적인 변경 내용
- 사용자 코드 수정 필요 여부
- 성능상 개선점 또는 주의사항
- 호환성 문제 및 구체적인 대응 방안

## ⚠️ 업데이트 시 주의사항

실제 변경이 있는 파일들에 대해:
- 반드시 테스트해야 할 시나리오
- 업데이트 전 확인 사항
- 단계별 적용 권장사항
- **브라우저별 테스트 필수 목록** (호환성 문제 발견 시)

## 📈 종합 평가
- 변경 규모 (대/중/소)
- 사이드 이펙트 리스크 (높음/중간/낮음) **※ 호환성 문제 발견 시 높음으로 설정**
- 업데이트 권장도 (즉시/테스트 후/신중히)
- 핵심 확인 대상 파일들

## 💡 결론 및 권장사항
- 주요 사이드 이펙트 요약
- 안전한 업데이트 전략
- 필수 확인 사항
- **즉시 수정이 필요한 호환성 문제** (발견 시)

**분석할 diff 데이터:**
{}",
        project, from_tag, to_tag,
        project, from_tag, to_tag,
        project, from_tag, to_tag,
        analysis_content
    );
    
    // 먼저 GPT-4 Turbo 시도
    let mut request = OpenAIRequest {
        model: "gpt-4-turbo".to_string(),
        messages: vec![
            Message {
                role: "user".to_string(),
                content: prompt.clone(),
            }
        ],
        max_tokens: 4000,
        temperature: 0.3,
    };
    
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;
    
    if response.status().is_success() {
        let openai_response: OpenAIResponse = response.json().await?;
        
        if !openai_response.choices.is_empty() {
            return Ok(openai_response.choices[0].message.content.clone());
        }
    } else {
        let error_text = response.text().await?;
        
        // 토큰 제한 오류인 경우 GPT-3.5 Turbo로 fallback
        if error_text.contains("context_length_exceeded") || error_text.contains("maximum context length") {
            println!("GPT-4 Turbo 토큰 제한에 걸렸습니다. GPT-3.5 Turbo로 재시도합니다...");
            
            // 더 작은 요약으로 재시도
            let fallback_content = if estimate_tokens(&analysis_content) > 8000 {
                smart_summarize_diff(&analysis_content, 6000)
            } else {
                analysis_content
            };
            
            let fallback_prompt = format!(
                "{}의 {} → {} 변경사항을 라이브러리 사용자 관점에서 분석해주세요.

**분석 목적**: 라이브러리를 빌드 후 사용하는 개발자가 버전 업데이트 시 발생할 수 있는 사이드 이펙트를 사전에 파악하여 방지

**분석 기준**:
- Chromium M38+ 버전 기준 (구체적인 API별 호환성 체크 필수)
- 라이브러리 빌드 후 사용자에게 실제 영향을 주는 변경사항만 분석
- 코드 스타일, 주석 등 동작에 영향 없는 변경사항은 제외
- API 변경, 동작 로직 변경, 성능 영향, 최적화 등 실질적 변경사항 중심
- 사용자 영향이 없더라도 동작 변경이 있으면 반드시 분석
- 각 변경사항마다 파일명과 실제 코드 변경 내용을 포함

**🚨 중요 API 호환성 체크리스트** (반드시 확인):
- **HTMLMediaElement.play()**: Chrome 50+에서 Promise 반환, 이전 버전(M38-M49)에서는 void 반환 → .catch() 사용 시 에러!
- **fetch()**: Chrome 42+ (M38에서는 사용 불가)
- **Promise**: Chrome 32+ (M38에서 지원)
- **async/await**: Chrome 55+ (M38에서는 사용 불가)
- **ResizeObserver**: Chrome 64+ (M38에서는 사용 불가)
- **IntersectionObserver**: Chrome 51+ (M38에서는 사용 불가)
- **Object.assign()**: Chrome 45+ (M38에서는 사용 불가)
- **Array.includes()**: Chrome 47+ (M38에서는 사용 불가)
- **Array.find()/findIndex()**: Chrome 45+ (M38에서는 사용 불가)
- **String.includes/startsWith/endsWith**: Chrome 41+ (M38에서는 사용 불가)
- **Map/Set**: Chrome 38+ (M38에서 지원)
- **for...of**: Chrome 38+ (M38에서 지원)

**중요**: 코드에서 이런 API들이 사용되면 반드시 브라우저 호환성을 체크하고, 문제가 있으면 **높은 리스크**로 분류하세요!

다음 형식으로 마크다운 분석 보고서를 작성해주세요:

# {} 변경사항 분석 ({} → {}) - 사이드 이펙트 분석

## 📊 개요
- 분석 대상: {} {} → {}
- 분석 목적: 라이브러리 사용자의 사이드 이펙트 방지
- 분석 기준: Chromium M38+ 버전 기준, 동작 변경 중심

## 🌐 크로스브라우징 영향 분석 (Chromium M38+ 기준)

실제 동작 변경이 있는 파일들을 분석하여 각 변경사항별로:
- 변경된 파일명과 구체적인 코드 변경 내용
- **구체적인 브라우저 호환성 문제** (상기 체크리스트 기준으로 정확히 분석)
- 호환성 문제가 있다면 **어떤 브라우저 버전에서 에러가 발생하는지** 명시
- 안전한 코딩 패턴 제시

### 🚨 호환성 경고 (발견 시)
각 문제별로:
**문제 코드**: `구체적인 코드`
**문제점**: Chrome M38-M49에서 HTMLMediaElement.play()는 void를 반환하므로 .catch() 호출 시 TypeError 발생
**안전한 코드**:
```javascript
const playPromise = media.play();
if (playPromise !== undefined) {{
  playPromise.catch(/* 에러 처리 */);
}}
```

## 🎬 미디어 재생 영향 분석

미디어 재생 관련 변경사항이 있다면:
- 변경된 파일명과 재생 로직 변경 내용
- 관련 미디어 기술 배경 설명 (코덱, 스트리밍, DRM 등)
- MediaSource, HTMLMediaElement 등 미디어 API 사용 여부
- 재생 품질, 성능, 안정성에 미치는 실제 영향
- 미디어 API 호환성 문제 (위 체크리스트 기준)

## 🔧 라이브러리 사용자 영향 분석

API 변경, 동작 변경, 성능 최적화 등이 있다면:
- 변경된 파일명과 구체적인 변경 내용
- 사용자 코드 수정 필요 여부
- 성능상 개선점 또는 주의사항
- 호환성 문제 및 구체적인 대응 방안

## ⚠️ 업데이트 시 주의사항

실제 변경이 있는 파일들에 대해:
- 반드시 테스트해야 할 시나리오
- 업데이트 전 확인 사항
- 단계별 적용 권장사항
- **브라우저별 테스트 필수 목록** (호환성 문제 발견 시)

## 📈 종합 평가
- 변경 규모 (대/중/소)
- 사이드 이펙트 리스크 (높음/중간/낮음) **※ 호환성 문제 발견 시 높음으로 설정**
- 업데이트 권장도 (즉시/테스트 후/신중히)
- 핵심 확인 대상 파일들

## 💡 결론 및 권장사항
- 주요 사이드 이펙트 요약
- 안전한 업데이트 전략
- 필수 확인 사항
- **즉시 수정이 필요한 호환성 문제** (발견 시)

**분석할 diff 데이터:**
{}",
                project, from_tag, to_tag,
                project, from_tag, to_tag,
                project, from_tag, to_tag,
                fallback_content
            );
            
            request.model = "gpt-3.5-turbo".to_string();
            request.messages[0].content = fallback_prompt;
            request.max_tokens = 2000;
            
            let fallback_response = client
                .post("https://api.openai.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;
            
            if fallback_response.status().is_success() {
                let fallback_result: OpenAIResponse = fallback_response.json().await?;
                
                if !fallback_result.choices.is_empty() {
                    println!("GPT-3.5 Turbo로 분석 완료!");
                    return Ok(fallback_result.choices[0].message.content.clone());
                }
            }
        }
        
        return Err(anyhow!("OpenAI API 요청 실패: {}", error_text));
    }
    
    Err(anyhow!("OpenAI API에서 응답을 받지 못했습니다"))
}

async fn analyze_commit_with_openai(diff_content: &str, api_key: &str, project: &str, commit_hash: &str) -> Result<String> {
    println!("OpenAI API로 커밋 분석 중...");
    
    let client = Client::new();
    
    // 프롬프트 토큰 추정 (약 800 토큰)
    let prompt_base_tokens = 800;
    let max_content_tokens = 120000 - prompt_base_tokens - 4000; // GPT-4 Turbo: 128k, 응답용 4k 예약
    
    // diff 내용 처리
    let analysis_content = if estimate_tokens(diff_content) > max_content_tokens {
        println!("Diff 내용이 큽니다. 스마트 요약해서 분석합니다...");
        smart_summarize_diff(diff_content, max_content_tokens)
    } else {
        diff_content.to_string()
    };
    
    println!("예상 토큰 사용량: {} / 128,000", estimate_tokens(&analysis_content) + prompt_base_tokens);
    
    let prompt = format!(
        "{}의 커밋 {} 변경사항을 라이브러리 사용자 관점에서 분석해주세요.

**분석 목적**: 라이브러리를 빌드 후 사용하는 개발자가 해당 커밋으로 인한 사이드 이펙트를 사전에 파악하여 방지

**분석 기준**:
- Chromium M38+ 버전 기준 (구체적인 API별 호환성 체크 필수)
- 라이브러리 빌드 후 사용자에게 실제 영향을 주는 변경사항만 분석
- 코드 스타일, 주석 등 동작에 영향 없는 변경사항은 제외
- API 변경, 동작 로직 변경, 성능 영향, 최적화 등 실질적 변경사항 중심
- 사용자 영향이 없더라도 동작 변경이 있으면 반드시 분석
- 각 변경사항마다 파일명과 실제 코드 변경 내용을 포함

**🚨 중요 API 호환성 체크리스트** (반드시 확인):
- **HTMLMediaElement.play()**: Chrome 50+에서 Promise 반환, 이전 버전(M38-M49)에서는 void 반환 → .catch() 사용 시 에러!
- **fetch()**: Chrome 42+ (M38에서는 사용 불가)
- **Promise**: Chrome 32+ (M38에서 지원)
- **async/await**: Chrome 55+ (M38에서는 사용 불가)
- **ResizeObserver**: Chrome 64+ (M38에서는 사용 불가)
- **IntersectionObserver**: Chrome 51+ (M38에서는 사용 불가)
- **Object.assign()**: Chrome 45+ (M38에서는 사용 불가)
- **Array.includes()**: Chrome 47+ (M38에서는 사용 불가)
- **Array.find()/findIndex()**: Chrome 45+ (M38에서는 사용 불가)
- **String.includes/startsWith/endsWith**: Chrome 41+ (M38에서는 사용 불가)
- **Map/Set**: Chrome 38+ (M38에서 지원)
- **for...of**: Chrome 38+ (M38에서 지원)

**중요**: 코드에서 이런 API들이 사용되면 반드시 브라우저 호환성을 체크하고, 문제가 있으면 **높은 리스크**로 분류하세요!

다음 형식으로 마크다운 분석 보고서를 작성해주세요:

# {} 커밋 {} 변경사항 분석 - 사이드 이펙트 분석

## 📊 개요
- 분석 대상: {} 커밋 {}
- 분석 목적: 라이브러리 사용자의 사이드 이펙트 방지
- 분석 기준: Chromium M38+ 버전 기준, 동작 변경 중심

## 🌐 크로스브라우징 영향 분석 (Chromium M38+ 기준)

실제 동작 변경이 있는 파일들을 분석하여 각 변경사항별로:
- 변경된 파일명과 구체적인 코드 변경 내용
- **구체적인 브라우저 호환성 문제** (상기 체크리스트 기준으로 정확히 분석)
- 호환성 문제가 있다면 **어떤 브라우저 버전에서 에러가 발생하는지** 명시
- 안전한 코딩 패턴 제시

### 🚨 호환성 경고 (발견 시)
각 문제별로:
**문제 코드**: `구체적인 코드`
**문제점**: Chrome M38-M49에서 HTMLMediaElement.play()는 void를 반환하므로 .catch() 호출 시 TypeError 발생
**안전한 코드**:
```javascript
const playPromise = media.play();
if (playPromise !== undefined) {{
  playPromise.catch(/* 에러 처리 */);
}}
```

## 🎬 미디어 재생 영향 분석

미디어 재생 관련 변경사항이 있다면:
- 변경된 파일명과 재생 로직 변경 내용
- 관련 미디어 기술 배경 설명 (코덱, 스트리밍, DRM 등)
- MediaSource, HTMLMediaElement 등 미디어 API 사용 여부
- 재생 품질, 성능, 안정성에 미치는 실제 영향
- 미디어 API 호환성 문제 (위 체크리스트 기준)

## 🔧 라이브러리 사용자 영향 분석

API 변경, 동작 변경, 성능 최적화 등이 있다면:
- 변경된 파일명과 구체적인 변경 내용
- 사용자 코드 수정 필요 여부
- 성능상 개선점 또는 주의사항
- 호환성 문제 및 구체적인 대응 방안

## ⚠️ 커밋 적용 시 주의사항

실제 변경이 있는 파일들에 대해:
- 반드시 테스트해야 할 시나리오
- 커밋 적용 전 확인 사항
- 단계별 적용 권장사항

## 📈 종합 평가
- 변경 규모 (대/중/소)
- 사이드 이펙트 리스크 (높음/중간/낮음) **※ 호환성 문제 발견 시 높음으로 설정**
- 업데이트 권장도 (즉시/테스트 후/신중히)
- 핵심 확인 대상 파일들

## 💡 결론 및 권장사항
- 주요 사이드 이펙트 요약
- 안전한 적용 전략
- 필수 확인 사항
- **즉시 수정이 필요한 호환성 문제** (발견 시)

**분석할 커밋 데이터:**
{}",
        project, commit_hash,
        project, commit_hash,
        project, commit_hash,
        analysis_content
    );
    
    // 먼저 GPT-4 Turbo 시도
    let mut request = OpenAIRequest {
        model: "gpt-4-turbo".to_string(),
        messages: vec![
            Message {
                role: "user".to_string(),
                content: prompt.clone(),
            }
        ],
        max_tokens: 4000,
        temperature: 0.3,
    };
    
    let response = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request)
        .send()
        .await?;
    
    if response.status().is_success() {
        let openai_response: OpenAIResponse = response.json().await?;
        
        if !openai_response.choices.is_empty() {
            return Ok(openai_response.choices[0].message.content.clone());
        }
    } else {
        let error_text = response.text().await?;
        
        // 토큰 제한 오류인 경우 GPT-3.5 Turbo로 fallback
        if error_text.contains("context_length_exceeded") || error_text.contains("maximum context length") {
            println!("GPT-4 Turbo 토큰 제한에 걸렸습니다. GPT-3.5 Turbo로 재시도합니다...");
            
            // 더 작은 요약으로 재시도
            let fallback_content = if estimate_tokens(&analysis_content) > 8000 {
                smart_summarize_diff(&analysis_content, 6000)
            } else {
                analysis_content
            };
            
            let fallback_prompt = format!(
                "{}의 커밋 {} 변경사항을 라이브러리 사용자 관점에서 분석해주세요.

**분석 목적**: 라이브러리를 빌드 후 사용하는 개발자가 해당 커밋으로 인한 사이드 이펙트를 사전에 파악하여 방지

**분석 기준**:
- Chromium M38+ 버전 기준 (구체적인 API별 호환성 체크 필수)
- 라이브러리 빌드 후 사용자에게 실제 영향을 주는 변경사항만 분석
- 코드 스타일, 주석 등 동작에 영향 없는 변경사항은 제외
- API 변경, 동작 로직 변경, 성능 영향, 최적화 등 실질적 변경사항 중심
- 사용자 영향이 없더라도 동작 변경이 있으면 반드시 분석
- 각 변경사항마다 파일명과 실제 코드 변경 내용을 포함

**🚨 중요 API 호환성 체크리스트** (반드시 확인):
- **HTMLMediaElement.play()**: Chrome 50+에서 Promise 반환, 이전 버전(M38-M49)에서는 void 반환 → .catch() 사용 시 에러!
- **fetch()**: Chrome 42+ (M38에서는 사용 불가)
- **Promise**: Chrome 32+ (M38에서 지원)
- **async/await**: Chrome 55+ (M38에서는 사용 불가)
- **ResizeObserver**: Chrome 64+ (M38에서는 사용 불가)
- **IntersectionObserver**: Chrome 51+ (M38에서는 사용 불가)
- **Object.assign()**: Chrome 45+ (M38에서는 사용 불가)
- **Array.includes()**: Chrome 47+ (M38에서는 사용 불가)
- **Array.find()/findIndex()**: Chrome 45+ (M38에서는 사용 불가)
- **String.includes/startsWith/endsWith**: Chrome 41+ (M38에서는 사용 불가)
- **Map/Set**: Chrome 38+ (M38에서 지원)
- **for...of**: Chrome 38+ (M38에서 지원)

**중요**: 코드에서 이런 API들이 사용되면 반드시 브라우저 호환성을 체크하고, 문제가 있으면 **높은 리스크**로 분류하세요!

다음 형식으로 마크다운 분석 보고서를 작성해주세요:

# {} 커밋 {} 변경사항 분석 - 사이드 이펙트 분석

## 📊 개요
- 분석 대상: {} 커밋 {}
- 분석 목적: 라이브러리 사용자의 사이드 이펙트 방지
- 분석 기준: Chromium M38+ 버전 기준, 동작 변경 중심

## 🌐 크로스브라우징 영향 분석 (Chromium M38+ 기준)

실제 동작 변경이 있는 파일들을 분석하여 각 변경사항별로:
- 변경된 파일명과 구체적인 코드 변경 내용
- **구체적인 브라우저 호환성 문제** (상기 체크리스트 기준으로 정확히 분석)
- 호환성 문제가 있다면 **어떤 브라우저 버전에서 에러가 발생하는지** 명시
- 안전한 코딩 패턴 제시

### 🚨 호환성 경고 (발견 시)
각 문제별로:
**문제 코드**: `구체적인 코드`
**문제점**: Chrome M38-M49에서 HTMLMediaElement.play()는 void를 반환하므로 .catch() 호출 시 TypeError 발생
**안전한 코드**:
```javascript
const playPromise = media.play();
if (playPromise !== undefined) {{
  playPromise.catch(/* 에러 처리 */);
}}
```

## 🎬 미디어 재생 영향 분석

미디어 재생 관련 변경사항이 있다면:
- 변경된 파일명과 재생 로직 변경 내용
- 관련 미디어 기술 배경 설명 (코덱, 스트리밍, DRM 등)
- MediaSource, HTMLMediaElement 등 미디어 API 사용 여부
- 재생 품질, 성능, 안정성에 미치는 실제 영향
- 미디어 API 호환성 문제 (위 체크리스트 기준)

## 🔧 라이브러리 사용자 영향 분석

API 변경, 동작 변경, 성능 최적화 등이 있다면:
- 변경된 파일명과 구체적인 변경 내용
- 사용자 코드 수정 필요 여부
- 성능상 개선점 또는 주의사항
- 호환성 문제 및 구체적인 대응 방안

## ⚠️ 커밋 적용 시 주의사항

실제 변경이 있는 파일들에 대해:
- 반드시 테스트해야 할 시나리오
- 커밋 적용 전 확인 사항
- 단계별 적용 권장사항

## 📈 종합 평가
- 변경 규모 (대/중/소)
- 사이드 이펙트 리스크 (높음/중간/낮음) **※ 호환성 문제 발견 시 높음으로 설정**
- 업데이트 권장도 (즉시/테스트 후/신중히)
- 핵심 확인 대상 파일들

## 💡 결론 및 권장사항
- 주요 사이드 이펙트 요약
- 안전한 적용 전략
- 필수 확인 사항
- **즉시 수정이 필요한 호환성 문제** (발견 시)

**분석할 커밋 데이터:**
{}",
                project, commit_hash,
                project, commit_hash,
                project, commit_hash,
                fallback_content
            );
            
            request.model = "gpt-3.5-turbo".to_string();
            request.messages[0].content = fallback_prompt;
            request.max_tokens = 2000;
            
            let fallback_response = client
                .post("https://api.openai.com/v1/chat/completions")
                .header("Authorization", format!("Bearer {}", api_key))
                .header("Content-Type", "application/json")
                .json(&request)
                .send()
                .await?;
            
            if fallback_response.status().is_success() {
                let fallback_result: OpenAIResponse = fallback_response.json().await?;
                
                if !fallback_result.choices.is_empty() {
                    println!("GPT-3.5 Turbo로 분석 완료!");
                    return Ok(fallback_result.choices[0].message.content.clone());
                }
            }
        }
        
        return Err(anyhow!("OpenAI API 요청 실패: {}", error_text));
    }
    
    Err(anyhow!("OpenAI API에서 응답을 받지 못했습니다"))
}

fn save_diff_to_file(diff_content: &str, filename: &str) -> Result<()> {
    fs::write(filename, diff_content)?;
    println!("Git diff가 {}에 저장되었습니다.", filename);
    Ok(())
}

fn save_summary_to_file(summary: &str, filename: &str) -> Result<()> {
    fs::write(filename, summary)?;
    println!("분석 요약이 {}에 저장되었습니다.", filename);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    dotenv().ok();
    
    let args = Args::parse();
    
    // 인자 유효성 검증
    if args.commit.is_some() && (args.from_tag.is_some() || args.to_tag.is_some()) {
        return Err(anyhow!("커밋 분석(-c)과 태그 간 분석(-f, -t)을 동시에 사용할 수 없습니다."));
    }
    
    if args.commit.is_none() && (args.from_tag.is_none() || args.to_tag.is_none()) {
        return Err(anyhow!("태그 간 분석을 위해서는 -f (from_tag)와 -t (to_tag) 모두 필요하거나, 커밋 분석을 위해서는 -c (commit)이 필요합니다."));
    }
    
    // OpenAI API 키 확인
    let api_key = env::var("OPENAI_API_KEY")
        .map_err(|_| anyhow!("OPENAI_API_KEY 환경변수가 설정되지 않았습니다. .env 파일을 확인해주세요."))?;
    
    if api_key == "your_openai_api_key_here" {
        return Err(anyhow!("OPENAI_API_KEY를 실제 API 키로 변경해주세요."));
    }
    
    // 프로젝트 경로 설정
    let project_path = args.path.unwrap_or_else(|| {
        format!("./repositories/{}", args.project)
    });
    
    // 프로젝트 경로 존재 확인
    if !Path::new(&project_path).exists() {
        return Err(anyhow!("프로젝트 경로가 존재하지 않습니다: {}", project_path));
    }
    
    // reports 디렉토리 생성
    fs::create_dir_all("reports")?;
    
    println!("프로젝트: {}", args.project);
    println!("프로젝트 경로: {}", project_path);
    
    let (diff_content, diff_filename, summary_filename, analysis_title, from_ref, to_ref) = if let Some(commit) = &args.commit {
        // 커밋 분석 모드
        println!("커밋: {}", commit);
        
        let diff_filename = format!("reports/{}_commit_{}_diff.txt", args.project, commit);
        let summary_filename = format!("reports/{}_commit_{}_summary.md", args.project, commit);
        
        let diff_content = get_commit_diff(&project_path, commit).await?;
        let analysis_title = format!("{} 커밋 {} 변경사항 분석", args.project, commit);
        
        (diff_content, diff_filename, summary_filename, analysis_title, commit.clone(), "".to_string())
    } else {
        // 태그 간 분석 모드
        let from_tag = args.from_tag.as_ref().unwrap();
        let to_tag = args.to_tag.as_ref().unwrap();
        
        println!("이전 태그: {}", from_tag);
        println!("이후 태그: {}", to_tag);
        
        let diff_filename = format!("reports/{}_{}_{}_diff.txt", args.project, from_tag, to_tag);
        let summary_filename = format!("reports/{}_{}_{}_summary.md", args.project, from_tag, to_tag);
        
        let diff_content = get_git_diff(&project_path, from_tag, to_tag).await?;
        let analysis_title = format!("{} 변경사항 분석 ({} → {})", args.project, from_tag, to_tag);
        
        (diff_content, diff_filename, summary_filename, analysis_title, from_tag.clone(), to_tag.clone())
    };
    
    if diff_content.trim().is_empty() {
        if args.commit.is_some() {
            println!("해당 커밋에 변경사항이 없습니다.");
        } else {
            println!("두 태그 간에 변경사항이 없습니다.");
        }
        return Ok(());
    }
    
    // Diff를 파일로 저장
    save_diff_to_file(&diff_content, &diff_filename)?;
    
    // OpenAI API로 분석 (개선된 프롬프트)
    let summary = if args.commit.is_some() {
        analyze_commit_with_openai(&diff_content, &api_key, &args.project, &from_ref).await?
    } else {
        analyze_diff_with_openai(&diff_content, &api_key, &args.project, &from_ref, &to_ref).await?
    };
    
    // 요약을 마크다운 파일로 저장
    save_summary_to_file(&summary, &summary_filename)?;
    
    println!("\n분석 완료!");
    println!("Git diff 파일: {}", diff_filename);
    println!("요약 파일: {}", summary_filename);
    
    Ok(())
} 