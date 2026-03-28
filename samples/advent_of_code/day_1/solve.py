lines = open('input.txt').read().strip().split('\n')
pos, count = 50, 0
for line in lines:
    line = line.strip()
    if not line:
        continue
    dir_c = line[0]
    dist = int(line[1:])
    if dir_c == 'R':
        pos = (pos + dist) % 100
    else:
        pos = (pos - dist + 10000) % 100
    if pos == 0:
        count += 1
print(f'Answer: {count}')
print(f'Final position: {pos}')
