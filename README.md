# Git Diff Analyzer

오픈소스 프로젝트의 태그 간 변경점을 분석하여 OpenAI API로 요약하는 Rust 도구입니다.

## 설치 및 설정

1. **의존성 설치**
   ```bash
   cd git-diff-analyzer
   cargo build
   ```

2. **환경변수 설정**
   
   `.env` 파일을 생성하고 OpenAI API 키를 설정하세요:
   ```
   OPENAI_API_KEY=your_actual_openai_api_key_here
   ```

3. **프로젝트 준비**
   
   분석할 오픈소스 프로젝트를 `repositories` 디렉토리에 클론하세요:
   ```bash
   cd ../repositories
   git clone https://github.com/username/project-name.git
   ```

## 사용 방법

```bash
cargo run -- --project <프로젝트명> --from-tag <이전태그> --to-tag <이후태그>
```

### 예시

```bash
# repositories/my-project에서 v1.0.0과 v1.1.0 간의 차이점 분석
cargo run -- --project my-project --from-tag v1.0.0 --to-tag v1.1.0

# 커스텀 프로젝트 경로 지정
cargo run -- --project my-project --from-tag v1.0.0 --to-tag v1.1.0 --path /path/to/project
```

## 출력 파일

- `<프로젝트명>_<이전태그>__<이후태그>.txt`: Git diff 원본
- `<프로젝트명>_<이전태그>__<이후태그>_summary.txt`: OpenAI 분석 요약

## 옵션

- `--project`, `-p`: 프로젝트 이름 (필수)
- `--from-tag`, `-f`: 이전 태그 (필수)
- `--to-tag`, `-t`: 이후 태그 (필수)
- `--path`: 프로젝트 경로 (선택사항, 기본값: ./repositories/{project})

## 주의사항

- OpenAI API 키가 필요합니다
- 프로젝트 디렉토리가 Git 저장소여야 합니다
- 지정한 태그가 존재해야 합니다 