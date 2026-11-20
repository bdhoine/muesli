# Writes a whisper.cpp ggml file with large-v3-turbo shapes (q5_0) and all-zero weights.
# Only for timing: compute cost does not depend on weight values; the output is meaningless.
import struct, sys, numpy as np
out = open(sys.argv[1], 'wb'); w = lambda f, *v: out.write(struct.pack(f, *v))
V, ACTX, S, H, AL, TCTX, TL, MELS = 51866, 1500, 1280, 20, 32, 448, 4, 128
w('<I', 0x67676d6c); w('<11i', V, ACTX, S, H, AL, TCTX, S, H, TL, MELS, 2008)
w('<ii', MELS, 201); out.write(np.zeros(MELS*201, np.float32).tobytes())
w('<i', 0)  # no vocab strings: the loader names the tokens itself
F32, F16, Q5 = 0, 1, 6
def t(name, ne, ty):
    n = int(np.prod(ne)); nb = name.encode()
    w('<iii', len(ne), len(nb), ty); [w('<i', x) for x in ne]; out.write(nb)
    size = {F32: 4*n, F16: 2*n, Q5: n//32*22}[ty]
    out.write(b'\0' * size)
t('encoder.positional_embedding', [S, ACTX], F32)
t('encoder.conv1.weight', [3, MELS, S], F16); t('encoder.conv1.bias', [1, S], F32)
t('encoder.conv2.weight', [3, S, S], F16); t('encoder.conv2.bias', [1, S], F32)
t('encoder.ln_post.weight', [S], F32); t('encoder.ln_post.bias', [S], F32)
def block(p, cross):
    t(p+'mlp_ln.weight',[S],F32); t(p+'mlp_ln.bias',[S],F32)
    t(p+'mlp.0.weight',[S,4*S],Q5); t(p+'mlp.0.bias',[4*S],F32)
    t(p+'mlp.2.weight',[4*S,S],Q5); t(p+'mlp.2.bias',[S],F32)
    for a in (['attn'] + (['cross_attn'] if cross else [])):
        t(p+a+'_ln.weight',[S],F32); t(p+a+'_ln.bias',[S],F32)
        t(p+a+'.query.weight',[S,S],Q5); t(p+a+'.query.bias',[S],F32)
        t(p+a+'.key.weight',[S,S],Q5)
        t(p+a+'.value.weight',[S,S],Q5); t(p+a+'.value.bias',[S],F32)
        t(p+a+'.out.weight',[S,S],Q5); t(p+a+'.out.bias',[S],F32)
for i in range(AL): block(f'encoder.blocks.{i}.', False)
t('decoder.positional_embedding', [S, TCTX], F32)
t('decoder.token_embedding.weight', [S, V], Q5)
t('decoder.ln.weight',[S],F32); t('decoder.ln.bias',[S],F32)
for i in range(TL): block(f'decoder.blocks.{i}.', True)
