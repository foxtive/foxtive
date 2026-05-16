import re

files = ['integration_tests.rs', 'concurrency_tests.rs']

for filename in files:
    with open(filename, 'r') as f:
        content = f.read()
    
    # Fix multi-line WorkerPool::with_concurrency calls
    pattern = r'(WorkerPool::with_concurrency\(\s*"[^"]+",\s*LoadBalancingStrategy::\w+,\s*\d+,)(\s*\))'
    replacement = r'\1\n        Arc::new(NoOpMetrics),\2'
    content = re.sub(pattern, replacement, content)
    
    with open(filename, 'w') as f:
        f.write(content)

print("Fixed test files")
