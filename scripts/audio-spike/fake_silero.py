import struct, random, sys
random.seed(1)
out = open(sys.argv[1], 'wb')
w = lambda f, *v: out.write(struct.pack(f, *v))
w('<I', 0x67676d6c)
t = b'silero-16k'; w('<i', len(t)); out.write(t)
w('<iii', 5, 1, 2)
w('<ii', 512, 64)
enc = [(129,128,3),(128,64,3),(64,64,3),(64,128,3)]
w('<i', 4)
for i,o,k in enc: w('<iii', i, o, k)
w('<iiii', 128, 128, 128, 1)
def tensor(name, ne, f16):
    n = 1
    for x in ne: n *= x
    nb = name.encode()
    w('<iii', len(ne), len(nb), 1 if f16 else 0)
    for x in ne: w('<i', x)
    out.write(nb)
    vals = [random.uniform(-0.05, 0.05) for _ in range(n)]
    out.write(struct.pack('<%d%s' % (n, 'e' if f16 else 'f'), *vals))
tensor('_model.stft.forward_basis_buffer', [256,1,258], True)
for j,(i,o,k) in enumerate(enc):
    tensor(f'_model.encoder.{j}.reparam_conv.weight', [k,i,o], True)
    tensor(f'_model.encoder.{j}.reparam_conv.bias', [o], False)
tensor('_model.decoder.rnn.weight_ih', [128,512], False)
tensor('_model.decoder.rnn.weight_hh', [128,512], False)
tensor('_model.decoder.rnn.bias_ih', [512], False)
tensor('_model.decoder.rnn.bias_hh', [512], False)
tensor('_model.decoder.decoder.2.weight', [128,1], True)
tensor('_model.decoder.decoder.2.bias', [1], False)
