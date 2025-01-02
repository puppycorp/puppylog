from tinygrad.tensor import Tensor
from tinygrad.nn import optim
from tinygrad.nn.state import get_parameters
import numpy as np

from gpt import GPT

# Example of small GPT model
vocab_size = 128  # Adjust based on tokenized log vocabulary
embed_dim = 128   # Embedding dimension
num_layers = 6    # Transformer layers
num_heads = 4     # Number of attention heads
seq_len = 128     # Max sequence length

model = GPT(vocab_size, embed_dim, num_layers, num_heads, seq_len)

# Dummy log dataset
def generate_data(batch_size, seq_len, vocab_size):
    return Tensor.uniform(batch_size, seq_len, low=0, high=vocab_size)

batch_size = 16
learning_rate = 1e-3
optimizer = optim.SGD(get_parameters(model), lr=learning_rate)

# Training loop
for epoch in range(10):
    x = generate_data(batch_size, seq_len, vocab_size)
    y = generate_data(batch_size, seq_len, vocab_size)  # Next-token predictions

    # Forward pass
    logits = model(x)
    loss = logits.logsoftmax(-1).mul(y.one_hot(vocab_size)).mean()

    # Backward pass
    optimizer.zero_grad()
    loss.backward()
    optimizer.step()

    print(f"Epoch {epoch + 1}, Loss: {loss.numpy()}")